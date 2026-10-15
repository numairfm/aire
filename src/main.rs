mod capture;
mod ui;
mod window;

use anyhow::Result;
use crossbeam_channel::bounded;
use gpui::{
    px, App, AppContext, Bounds, Point, Size, WindowBackgroundAppearance, WindowBounds,
    WindowDecorations, WindowKind, WindowOptions,
};
use gpui_platform::application;

use crate::{
    capture::{start_window_capture, CaptureEvent},
    ui::MirrorView,
    window::configure_platform_window,
};

fn main() -> Result<()> {
    const APP_ID: &str = "aire";

    // 1. Configure platform window rules (KWin keep-above, noborder, skiptaskbar)
    if let Err(err) = configure_platform_window(APP_ID) {
        eprintln!("[WINDOW] Platform window setup warning: {:?}", err);
    }

    // 2. Set up communication channel for capture events
    let (event_tx, event_rx) = bounded::<CaptureEvent>(4);

    // 3. Start window capture backend (pops up native KDE window picker)
    let capture = start_window_capture(event_tx)?;

    // 4. Run GPUI application
    application().run(move |cx: &mut App| {
        let display_bounds = cx.displays().first().map(|d| d.bounds()).unwrap_or_else(|| {
            Bounds {
                origin: Point::new(px(0.0), px(0.0)),
                size: Size::new(px(1920.0), px(1080.0)),
            }
        });

        // Default mirror size: 400px wide, 16:9 aspect ratio (225px)
        let win_width = px(400.0);
        let win_height = px(225.0);

        // Center on screen on app open as requested
        let origin_x = display_bounds.origin.x + (display_bounds.size.width - win_width) / 2.0;
        let origin_y = display_bounds.origin.y + (display_bounds.size.height - win_height) / 2.0;

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::new(origin_x, origin_y),
                size: Size::new(win_width, win_height),
            })),
            titlebar: None,
            app_id: Some(APP_ID.to_string()),
            window_decorations: Some(WindowDecorations::Client),
            window_background: WindowBackgroundAppearance::Transparent,
            kind: WindowKind::Normal,
            is_movable: true,
            is_resizable: true,
            window_min_size: Some(Size::new(px(180.0), px(36.0))),
            ..Default::default()
        };

        cx.open_window(options, move |_, cx| {
            cx.activate(false);
            cx.new(|cx| MirrorView::new(cx, capture, event_rx))
        })
        .expect("Failed to open aire mirror window");
    });

    Ok(())
}
