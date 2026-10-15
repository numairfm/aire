use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{
    div, prelude::*, px, rgb, rgba, size, App, Bounds, Context, FontWeight,
    Point, Render, RenderImage, ResizeEdge, Size, Window, WindowBackgroundAppearance,
    WindowBounds, WindowDecorations, WindowKind, WindowOptions,
};
use gpui_platform::application;
use image::{Frame, ImageBuffer, Rgba};
use smallvec::smallvec;

struct PerfStats {
    fps: f64,
    frame_upload_ms: f64,
    cpu_usage_pct: f64,
    rss_mb: f64,
}

struct SpikePerfView {
    current_image: Option<Arc<RenderImage>>,
    stats: PerfStats,
    is_minimized: bool,
}

impl SpikePerfView {
    fn new(cx: &mut Context<Self>) -> Self {
        let width = 1920;
        let height = 1080;

        cx.spawn(async move |this, cx| {
            let mut frame_idx: u64 = 0;
            let mut last_fps_time = Instant::now();
            let mut frames_this_sec = 0;
            let mut last_cpu_sample = Instant::now();
            let mut last_cpu_utime = 0u64;
            let mut last_cpu_stime = 0u64;

            let interval = Duration::from_micros(16666); // ~60fps target
            let mut next_tick = Instant::now();

            loop {
                // Check if minimized (pause capture/upload)
                let is_minimized = this
                    .read_with(cx, |view, _| view.is_minimized)
                    .unwrap_or(false);
                if is_minimized {
                    cx.background_executor().timer(Duration::from_millis(100)).await;
                    continue;
                }

                // Generate a realistic dynamic 1080p frame (1920x1080 = ~8.3MB)
                // Color gradient with moving sweep bar and visual activity
                let mut raw_buf = vec![0u8; (width * height * 4) as usize];
                let sweep = ((frame_idx * 16) % (width as u64)) as usize;

                for y in 0..height as usize {
                    let y_offset = y * (width as usize) * 4;
                    let base_r = ((y * 255) / (height as usize)) as u8;
                    for x in 0..width as usize {
                        let idx = y_offset + x * 4;
                        let is_bar = x >= sweep && x < sweep + 40;
                        raw_buf[idx] = if is_bar { 255 } else { base_r / 2 };
                        raw_buf[idx + 1] = if is_bar { 220 } else { 130 };
                        raw_buf[idx + 2] = if is_bar { 60 } else { 210 };
                        raw_buf[idx + 3] = 255;
                    }
                }

                let img_buffer: ImageBuffer<Rgba<u8>, Vec<u8>> =
                    ImageBuffer::from_raw(width, height, raw_buf).expect("valid buffer size");

                let upload_start = Instant::now();
                let render_img = Arc::new(RenderImage::new(smallvec![Frame::new(img_buffer)]));
                let upload_elapsed = upload_start.elapsed().as_secs_f64() * 1000.0;

                frames_this_sec += 1;
                let now = Instant::now();

                let mut current_fps = 0.0;
                if now.duration_since(last_fps_time) >= Duration::from_secs(1) {
                    current_fps = frames_this_sec as f64
                        / now.duration_since(last_fps_time).as_secs_f64();
                    frames_this_sec = 0;
                    last_fps_time = now;
                }

                // Measure process CPU % and memory RSS
                let mut cpu_pct = 0.0;
                let mut rss = 0.0;
                if now.duration_since(last_cpu_sample) >= Duration::from_millis(500) {
                    if let Ok(stat) = std::fs::read_to_string("/proc/self/stat") {
                        let parts: Vec<&str> = stat.split_whitespace().collect();
                        if parts.len() > 14 {
                            let utime: u64 = parts[13].parse().unwrap_or(0);
                            let stime: u64 = parts[14].parse().unwrap_or(0);
                            let elapsed_sec = now.duration_since(last_cpu_sample).as_secs_f64();
                            let total_ticks = (utime.saturating_sub(last_cpu_utime)
                                + stime.saturating_sub(last_cpu_stime)) as f64;
                            cpu_pct = (total_ticks / 100.0) / elapsed_sec * 100.0;
                            last_cpu_utime = utime;
                            last_cpu_stime = stime;
                        }
                    }
                    if let Ok(statm) = std::fs::read_to_string("/proc/self/statm") {
                        let parts: Vec<&str> = statm.split_whitespace().collect();
                        if parts.len() > 1 {
                            let resident_pages: u64 = parts[1].parse().unwrap_or(0);
                            rss = (resident_pages * 4096) as f64 / (1024.0 * 1024.0);
                        }
                    }
                    last_cpu_sample = now;

                    if current_fps > 0.0 {
                        eprintln!(
                            "[SPIKE PROBE] 1080p@60fps | Achieved: {:.1} FPS | Upload: {:.2} ms | CPU: {:.1}% | RSS: {:.1} MB",
                            current_fps, upload_elapsed, cpu_pct, rss
                        );
                    }
                }

                let _ = this.update(cx, |view, cx| {
                    view.current_image = Some(render_img);
                    if current_fps > 0.0 {
                        view.stats.fps = current_fps;
                    }
                    view.stats.frame_upload_ms = upload_elapsed;
                    if cpu_pct > 0.0 {
                        view.stats.cpu_usage_pct = cpu_pct;
                    }
                    if rss > 0.0 {
                        view.stats.rss_mb = rss;
                    }
                    cx.notify();
                });

                frame_idx += 1;
                next_tick += interval;
                let now = Instant::now();
                if next_tick > now {
                    cx.background_executor().timer(next_tick - now).await;
                } else {
                    next_tick = now;
                }
            }
        })
        .detach();

        Self {
            current_image: None,
            stats: PerfStats {
                fps: 0.0,
                frame_upload_ms: 0.0,
                cpu_usage_pct: 0.0,
                rss_mb: 0.0,
            },
            is_minimized: false,
        }
    }
}

impl Render for SpikePerfView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.is_minimized {
            // Minimized pill mode: compact 36px pill with frosty glass styling
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgba(0xffffffea))
                .rounded(px(18.0))
                .border_1()
                .border_color(rgba(0xffffffcc))
                .shadow_lg()
                .cursor_pointer()
                .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                    window.start_window_move();
                })
                .id("restore-pill")
                .on_click(cx.listener(|this, _, window, cx| {
                    this.is_minimized = false;
                    window.resize(size(px(400.0), px(225.0)));
                    cx.notify();
                }))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0x222222))
                        .child("aire • click to restore"),
                )
                .into_any_element();
        }

        // Full mirror HUD mode with declarative hover styling
        div()
            .group("hud")
            .size_full()
            .relative()
            // Frosty light glass styling
            .bg(rgba(0xffffff28))
            .rounded(px(10.0))
            .border_1()
            .border_color(rgba(0xffffff48))
            .shadow_xl()
            .overflow_hidden()
            // Click-drag anywhere to move window
            .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                window.start_window_move();
            })
            // Video / Mirror Content Layer (eases to 35% on hover)
            .child(
                div()
                    .size_full()
                    .opacity(1.0)
                    .group_hover("hud", |s| s.opacity(0.35))
                    .child(if let Some(ref img_data) = self.current_image {
                        gpui::img(img_data.clone())
                            .size_full()
                            .into_any_element()
                    } else {
                        div()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(rgb(0x555555))
                            .child("Initializing stream...")
                            .into_any_element()
                    }),
            )
            // Frosty Glass Controls Overlay (Appears on Hover)
            .child(
                div()
                    .absolute()
                    .top_2()
                    .right_2()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .opacity(0.0)
                    .group_hover("hud", |s| s.opacity(1.0))
                    // Minimize button
                    .child(
                        div()
                            .id("btn-minimize")
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(24.0))
                            .rounded(px(12.0))
                            .bg(rgba(0xffffffea))
                            .border_1()
                            .border_color(rgba(0x00000015))
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.is_minimized = true;
                                window.resize(size(px(180.0), px(36.0)));
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .w(px(10.0))
                                    .h(px(2.0))
                                    .bg(rgb(0x333333)),
                            ),
                    )
                    // Close button
                    .child(
                        div()
                            .id("btn-close")
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(24.0))
                            .rounded(px(12.0))
                            .bg(rgba(0xffffffea))
                            .border_1()
                            .border_color(rgba(0x00000015))
                            .cursor_pointer()
                            .on_click(|_, _, cx| {
                                cx.quit();
                            })
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(0x333333))
                                    .child("✕"),
                            ),
                    ),
            )
            // Diagnostics HUD Banner (Bottom-left)
            .child(
                div()
                    .absolute()
                    .bottom_2()
                    .left_2()
                    .px_2()
                    .py_1()
                    .rounded(px(6.0))
                    .bg(rgba(0x000000a0))
                    .text_xs()
                    .text_color(rgb(0xffffff))
                    .opacity(0.6)
                    .group_hover("hud", |s| s.opacity(0.95))
                    .child(format!(
                        "1080p60 | {:.0} fps | up: {:.1}ms | cpu: {:.1}% | rss: {:.0}MB",
                        self.stats.fps, self.stats.frame_upload_ms, self.stats.cpu_usage_pct, self.stats.rss_mb
                    )),
            )
            // Border Resize Handles
            // Left edge
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(px(6.0))
                    .cursor_col_resize()
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                        window.start_window_resize(ResizeEdge::Left);
                    }),
            )
            // Right edge
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right_0()
                    .w(px(6.0))
                    .cursor_col_resize()
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                        window.start_window_resize(ResizeEdge::Right);
                    }),
            )
            // Top edge
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(px(6.0))
                    .cursor_row_resize()
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                        window.start_window_resize(ResizeEdge::Top);
                    }),
            )
            // Bottom edge
            .child(
                div()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .h(px(6.0))
                    .cursor_row_resize()
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                        window.start_window_resize(ResizeEdge::Bottom);
                    }),
            )
            // Bottom-right corner
            .child(
                div()
                    .absolute()
                    .bottom_0()
                    .right_0()
                    .size(px(14.0))
                    .cursor_nwse_resize()
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                        window.start_window_resize(ResizeEdge::BottomRight);
                    }),
            )
            .into_any_element()
    }
}

fn configure_kwin_rules_if_needed() {
    let rule_id = "aire_mirror_rule";
    let _ = std::process::Command::new("kwriteconfig6")
        .args(&[
            "--file", "kwinrulesrc",
            "--group", rule_id,
            "--key", "Description", "aire window mirror",
        ])
        .status();
    let _ = std::process::Command::new("kwriteconfig6")
        .args(&[
            "--file", "kwinrulesrc",
            "--group", rule_id,
            "--key", "wmclass", "aire",
        ])
        .status();
    let _ = std::process::Command::new("kwriteconfig6")
        .args(&[
            "--file", "kwinrulesrc",
            "--group", rule_id,
            "--key", "wmclassmatch", "1",
        ])
        .status();
    let _ = std::process::Command::new("kwriteconfig6")
        .args(&[
            "--file", "kwinrulesrc",
            "--group", rule_id,
            "--key", "above", "true",
        ])
        .status();
    let _ = std::process::Command::new("kwriteconfig6")
        .args(&[
            "--file", "kwinrulesrc",
            "--group", rule_id,
            "--key", "aboverule", "3",
        ])
        .status();
    let _ = std::process::Command::new("kwriteconfig6")
        .args(&[
            "--file", "kwinrulesrc",
            "--group", rule_id,
            "--key", "noborder", "true",
        ])
        .status();
    let _ = std::process::Command::new("kwriteconfig6")
        .args(&[
            "--file", "kwinrulesrc",
            "--group", rule_id,
            "--key", "noborderrule", "3",
        ])
        .status();
    let _ = std::process::Command::new("kwriteconfig6")
        .args(&[
            "--file", "kwinrulesrc",
            "--group", rule_id,
            "--key", "skiptaskbar", "true",
        ])
        .status();
    let _ = std::process::Command::new("kwriteconfig6")
        .args(&[
            "--file", "kwinrulesrc",
            "--group", rule_id,
            "--key", "skiptaskbarrule", "3",
        ])
        .status();

    let _ = std::process::Command::new("qdbus-qt6")
        .args(&["org.kde.KWin", "/KWin", "reconfigure"])
        .status();
}

fn main() {
    configure_kwin_rules_if_needed();

    application().run(|cx: &mut App| {
        let display_bounds = cx.displays().first().map(|d| d.bounds()).unwrap_or_else(|| {
            Bounds {
                origin: Point::new(px(0.0), px(0.0)),
                size: Size::new(px(1920.0), px(1080.0)),
            }
        });

        let win_width = px(400.0);
        let win_height = px(225.0);

        // Center on screen
        let origin_x = display_bounds.origin.x + (display_bounds.size.width - win_width) / 2.0;
        let origin_y = display_bounds.origin.y + (display_bounds.size.height - win_height) / 2.0;

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::new(origin_x, origin_y),
                size: Size::new(win_width, win_height),
            })),
            titlebar: None,
            app_id: Some("aire".to_string()),
            window_decorations: Some(WindowDecorations::Client),
            window_background: WindowBackgroundAppearance::Transparent,
            kind: WindowKind::Normal,
            is_movable: true,
            is_resizable: true,
            ..Default::default()
        };

        cx.open_window(options, |_, cx| {
            cx.activate(false);
            cx.new(SpikePerfView::new)
        })
        .expect("Failed to open GPUI spike window");
    });
}
