mod capture;
mod ui;
mod window;

use anyhow::Result;
use gpui::{
    px, App, AppContext, Bounds, Point, Size, WindowBackgroundAppearance, WindowBounds,
    WindowDecorations, WindowKind, WindowOptions,
};
use gpui_platform::application;

use crate::{
    capture::{create_capture_channel, start_window_capture},
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
    let (event_tx, event_rx) = create_capture_channel();

    // 3. Start window capture backend (user picks window in native KDE portal dialog)
    // If user cancels the picker or closes it, exit cleanly without opening any window
    let Some((capture, initial_dims)) = start_window_capture(event_tx)? else {
        eprintln!("[AIRE] Window selection cancelled. Exiting.");
        return Ok(());
    };

    let (source_w, source_h) = initial_dims.unwrap_or((1920, 1080));
    let aspect_ratio = if source_h > 0 {
        source_w as f32 / source_h as f32
    } else {
        16.0 / 9.0
    };

    // Calculate initial mirror window size shaped to the source aspect ratio
    // Target ~400px width for standard/wide windows, or ~280px for vertical/portrait windows
    let win_width = if aspect_ratio < 0.85 {
        px(280.0) // compact width for tall/vertical windows
    } else if aspect_ratio > 2.2 {
        px(480.0) // wider for ultrawide windows
    } else {
        px(400.0) // standard width
    };
    let win_height = px((f32::from(win_width) / aspect_ratio).round().max(100.0));

    // 4. Run GPUI application now that window is selected
    application().run(move |cx: &mut App| {
        let display_bounds = cx.displays().first().map(|d| d.bounds()).unwrap_or_else(|| {
            Bounds {
                origin: Point::new(px(0.0), px(0.0)),
                size: Size::new(px(1920.0), px(1080.0)),
            }
        });

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
            cx.new(|cx| MirrorView::new(cx, capture, event_rx, aspect_ratio, source_w, source_h))
        })
        .expect("Failed to open aire mirror window");
    });

    Ok(())
}
