use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{
    div, prelude::*, px, rgb, rgba, size, Context, FontWeight, Render, RenderImage, ResizeEdge,
    ScrollWheelEvent, WeakEntity, Window,
};

use crate::capture::{CaptureEvent, CaptureReceiver, WindowCapture};

pub struct MirrorView {
    capture: Box<dyn WindowCapture>,
    current_image: Option<Arc<RenderImage>>,
    source_width: u32,
    source_height: u32,
    aspect_ratio: f32,
    fps: f64,
    is_minimized: bool,
    source_closed: bool,
    error_message: Option<String>,
    last_window_width: f32,
    last_window_height: f32,
    last_kwin_w: f32,
    last_kwin_h: f32,
    needs_aspect_adaptation: bool,
}

impl MirrorView {
    pub fn new(
        cx: &mut Context<Self>,
        capture: Box<dyn WindowCapture>,
        event_rx: CaptureReceiver,
        initial_aspect_ratio: f32,
        initial_width: u32,
        initial_height: u32,
    ) -> Self {
        // Spawn background listener for capture events (100% non-blocking)
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let mut frames_this_sec = 0;
            let mut last_fps_time = Instant::now();
            let last_frame_instant = Arc::new(parking_lot::Mutex::new(Instant::now()));

            // Periodic timer task to reset FPS to 0.0 when source is static
            let this_fps_watcher = this.clone();
            let last_frame_instant_checker = last_frame_instant.clone();
            let _fps_checker = cx.spawn(async move |cx| {
                loop {
                    cx.background_executor().timer(Duration::from_millis(500)).await;
                    let last_seen = *last_frame_instant_checker.lock();
                    let _ = this_fps_watcher.update(cx, |view, cx| {
                        if view.fps > 0.0 && last_seen.elapsed() > Duration::from_millis(1500) {
                            view.fps = 0.0;
                            cx.notify();
                        }
                    });
                }
            });

            while let Ok(event) = event_rx.recv().await {
                match event {
                    CaptureEvent::Frame(mut frame) => {
                        // Drain any backlog of frames to always render the freshest frame with zero latency
                        while let Ok(next_event) = event_rx.try_recv() {
                            match next_event {
                                CaptureEvent::Frame(f) => frame = f,
                                CaptureEvent::Resized { width, height } => {
                                    let _ = this.update(cx, |view, cx| {
                                        view.source_width = width;
                                        view.source_height = height;
                                        if height > 0 {
                                            let new_aspect = width as f32 / height as f32;
                                            if (view.aspect_ratio - new_aspect).abs() > 0.005 {
                                                view.aspect_ratio = new_aspect;
                                                view.needs_aspect_adaptation = true;
                                            }
                                        }
                                        cx.notify();
                                    });
                                }
                                CaptureEvent::SourceClosed => {
                                    let _ = this.update(cx, |view, cx| {
                                        view.source_closed = true;
                                        cx.notify();
                                    });
                                    return;
                                }
                                CaptureEvent::Error(msg) => {
                                    let _ = this.update(cx, |view, cx| {
                                        view.error_message = Some(msg);
                                        cx.notify();
                                    });
                                }
                            }
                        }

                        frames_this_sec += 1;
                        *last_frame_instant.lock() = Instant::now();
                        let now = Instant::now();
                        let mut new_fps = None;
                        if now.duration_since(last_fps_time) >= Duration::from_secs(1) {
                            let fps = frames_this_sec as f64
                                / now.duration_since(last_fps_time).as_secs_f64();
                            new_fps = Some(fps);
                            frames_this_sec = 0;
                            last_fps_time = now;
                        }

                        let _ = this.update(cx, |view, cx| {
                            view.current_image = Some(frame.render_image);
                            view.source_width = frame.width;
                            view.source_height = frame.height;
                            if frame.height > 0 {
                                let frame_aspect = frame.width as f32 / frame.height as f32;
                                if (view.aspect_ratio - frame_aspect).abs() > 0.005 {
                                    view.aspect_ratio = frame_aspect;
                                    view.needs_aspect_adaptation = true;
                                }
                            }
                            if let Some(fps) = new_fps {
                                view.fps = fps;
                            }
                            cx.notify();
                        });
                    }
                    CaptureEvent::Resized { width, height } => {
                        let _ = this.update(cx, |view, cx| {
                            view.source_width = width;
                            view.source_height = height;
                            if height > 0 {
                                let new_aspect = width as f32 / height as f32;
                                if (view.aspect_ratio - new_aspect).abs() > 0.005 {
                                    view.aspect_ratio = new_aspect;
                                    view.needs_aspect_adaptation = true;
                                }
                            }
                            cx.notify();
                        });
                    }
                    CaptureEvent::SourceClosed => {
                        let _ = this.update(cx, |view, cx| {
                            view.source_closed = true;
                            cx.notify();
                        });
                        break;
                    }
                    CaptureEvent::Error(msg) => {
                        let _ = this.update(cx, |view, cx| {
                            view.error_message = Some(msg);
                            cx.notify();
                        });
                    }
                }
            }
        })
        .detach();

        Self {
            capture,
            current_image: None,
            source_width: initial_width,
            source_height: initial_height,
            aspect_ratio: initial_aspect_ratio,
            fps: 0.0,
            is_minimized: false,
            source_closed: false,
            error_message: None,
            last_window_width: 0.0,
            last_window_height: 0.0,
            last_kwin_w: 0.0,
            last_kwin_h: 0.0,
            needs_aspect_adaptation: false,
        }
    }
}

impl Render for MirrorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 1. Maintain Locked Aspect Ratio during source resize and interactive window resizing
        if !self.is_minimized {
            let bounds = window.bounds();
            let raw_w = f32::from(bounds.size.width);
            let raw_h = f32::from(bounds.size.height);

            let aspect = if self.aspect_ratio > 0.05 {
                self.aspect_ratio
            } else {
                16.0 / 9.0
            };

            // Determine display bounds to establish maximum window boundaries
            let (disp_w, disp_h) = window
                .display(cx)
                .map(|d| {
                    (
                        f32::from(d.bounds().size.width),
                        f32::from(d.bounds().size.height),
                    )
                })
                .unwrap_or((1920.0, 1080.0));

            // Minimum size (at least 240px wide for landscape, or 240px tall for portrait)
            let (min_w, min_h) = if aspect >= 1.0 {
                let w = 240.0f32;
                let h = (w / aspect).round().max(135.0);
                (w, h)
            } else {
                let h = 240.0f32;
                let w = (h * aspect).round().max(160.0);
                (w, h)
            };

            // Maximum size (at most 75% of screen size, capped at 1440x850)
            let max_bound_w = (disp_w * 0.75).min(1440.0);
            let max_bound_h = (disp_h * 0.75).min(850.0);
            let (max_w, max_h) = if aspect >= 1.0 {
                let w = max_bound_w;
                let h = (w / aspect).round();
                if h > max_bound_h {
                    let h = max_bound_h;
                    let w = (h * aspect).round();
                    (w, h)
                } else {
                    (w, h)
                }
            } else {
                let h = max_bound_h;
                let w = (h * aspect).round();
                if w > max_bound_w {
                    let w = max_bound_w;
                    let h = (w / aspect).round();
                    (w, h)
                } else {
                    (w, h)
                }
            };

            if self.needs_aspect_adaptation {
                self.needs_aspect_adaptation = false;
                let w = raw_w.clamp(min_w, max_w);
                let h = (w / aspect).round().clamp(min_h, max_h);
                let w = (h * aspect).round();
                self.last_window_width = w;
                self.last_window_height = h;
                self.last_kwin_w = w;
                self.last_kwin_h = h;
                if (raw_w - w).abs() >= 1.0 || (raw_h - h).abs() >= 1.0 {
                    window.resize(size(px(w), px(h)));
                }
            } else if self.last_kwin_w > 0.0 && self.last_kwin_h > 0.0 {
                // If KWin's incoming configure size changed by >= 2px, enforce strict aspect ratio
                let kwin_input_changed = (raw_w - self.last_kwin_w).abs() >= 2.0
                    || (raw_h - self.last_kwin_h).abs() >= 2.0;

                if kwin_input_changed {
                    self.last_kwin_w = raw_w;
                    self.last_kwin_h = raw_h;

                    // Project (raw_w, raw_h) onto the aspect-ratio ray: (aspect, 1.0)
                    let t = (raw_w * aspect + raw_h) / (aspect * aspect + 1.0);
                    let target_h = t.round().clamp(min_h, max_h);
                    let target_w = (target_h * aspect).round().clamp(min_w, max_w);
                    let target_h = (target_w / aspect).round();

                    self.last_window_width = target_w;
                    self.last_window_height = target_h;

                    if (raw_w - target_w).abs() >= 1.0 || (raw_h - target_h).abs() >= 1.0 {
                        window.resize(size(px(target_w), px(target_h)));
                    }
                }
            } else {
                self.last_kwin_w = raw_w;
                self.last_kwin_h = raw_h;
                self.last_window_width = raw_w;
                self.last_window_height = raw_h;
            }
        }

        // 1. Minimized Pill State
        if self.is_minimized {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_between()
                .px_2()
                // Light frosty pill glass style
                .bg(rgba(0xffffffea))
                .rounded(px(18.0))
                .border_1()
                .border_color(rgba(0xffffffbb))
                .shadow_lg()
                // Left side: Drag handle to move the pill
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .cursor_grab()
                        .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                            cx.stop_propagation();
                            window.start_window_move();
                        })
                        .child(
                            div()
                                .size(px(8.0))
                                .rounded(px(4.0))
                                .bg(rgb(0x10b981)), // gentle emerald green pulse
                        )
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(0x374151))
                                .child("aire"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x9ca3af))
                                .child("⋮"),
                        ),
                )
                // Right side: Click to restore
                .child(
                    div()
                        .id("restore-pill-action")
                        .flex_1()
                        .flex()
                        .justify_end()
                        .cursor_pointer()
                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.is_minimized = false;
                            this.capture.resume();
                            window.resize(size(
                                px(this.last_window_width.max(240.0)),
                                px(this.last_window_height.max(135.0)),
                            ));
                            cx.notify();
                        }))
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(0x4b5563))
                                .hover(|s| s.text_color(rgb(0x111827)))
                                .child("restore ↗"),
                        ),
                )
                .into_any_element();
        }

        // 2. Source Window Closed State
        if self.source_closed {
            return div()
                .size_full()
                .relative()
                .bg(rgba(0xffffff33))
                .rounded(px(10.0))
                .border_1()
                .border_color(rgba(0xffffff55))
                .shadow_xl()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .p_4()
                .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                    window.start_window_move();
                })
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(0x1f2937))
                        .child("Source window closed"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x4b5563))
                        .child("The mirrored window was closed by the user."),
                )
                .child(
                    div()
                        .id("btn-exit-closed")
                        .mt_2()
                        .px_3()
                        .py_1()
                        .rounded(px(6.0))
                        .bg(rgba(0xffffffea))
                        .border_1()
                        .border_color(rgba(0x00000015))
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0x111827))
                        .cursor_pointer()
                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_click(|_, _, cx| {
                            cx.quit();
                        })
                        .child("Close aire"),
                )
                .into_any_element();
        }

        // 3. Error State (if picker was cancelled or failed)
        if let Some(ref err) = self.error_message {
            return div()
                .size_full()
                .relative()
                .bg(rgba(0xffffff33))
                .rounded(px(10.0))
                .border_1()
                .border_color(rgba(0xffffff55))
                .shadow_xl()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .p_4()
                .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                    window.start_window_move();
                })
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(0x991b1b))
                        .child("Capture could not start"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0x374151))
                        .child(err.clone()),
                )
                .child(
                    div()
                        .id("btn-exit-error")
                        .mt_2()
                        .px_3()
                        .py_1()
                        .rounded(px(6.0))
                        .bg(rgba(0xffffffea))
                        .border_1()
                        .border_color(rgba(0x00000015))
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0x111827))
                        .cursor_pointer()
                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                            cx.stop_propagation();
                        })
                        .on_click(|_, _, cx| {
                            cx.quit();
                        })
                        .child("Exit"),
                )
                .into_any_element();
        }

        // 4. Normal Active Mirror HUD
        let is_hovered = window.is_window_hovered();

        div()
            .id("mirror-root")
            .size_full()
            .relative()
            // Frosty light glass styling
            .bg(rgba(0xffffff28))
            .rounded(px(10.0))
            .border_1()
            .border_color(rgba(0xffffff50))
            .shadow_xl()
            .overflow_hidden()
            // Click-drag anywhere on the mirror body to move the window freely
            .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                window.start_window_move();
            })
            // Mouse scroll wheel scaling (strictly locked to aspect ratio)
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                if this.is_minimized {
                    return;
                }
                let delta_y = match event.delta {
                    gpui::ScrollDelta::Lines(lines) => lines.y * 15.0,
                    gpui::ScrollDelta::Pixels(pixels) => f32::from(pixels.y),
                };
                if delta_y.abs() < 0.5 {
                    return;
                }

                let aspect = if this.aspect_ratio > 0.05 {
                    this.aspect_ratio
                } else {
                    16.0 / 9.0
                };

                let bounds = window.bounds();
                let cur_w = f32::from(bounds.size.width);
                let cur_h = f32::from(bounds.size.height);

                let scale_factor = 1.0 + (delta_y / 350.0).clamp(-0.15, 0.15);
                let (disp_w, disp_h) = window
                    .display(cx)
                    .map(|d| {
                        (
                            f32::from(d.bounds().size.width),
                            f32::from(d.bounds().size.height),
                        )
                    })
                    .unwrap_or((1920.0, 1080.0));

                let (min_w, min_h) = if aspect >= 1.0 {
                    let w = 240.0f32;
                    let h = (w / aspect).round().max(135.0);
                    (w, h)
                } else {
                    let h = 240.0f32;
                    let w = (h * aspect).round().max(160.0);
                    (w, h)
                };

                let max_bound_w = (disp_w * 0.75).min(1440.0);
                let max_bound_h = (disp_h * 0.75).min(850.0);
                let (max_w, max_h) = if aspect >= 1.0 {
                    let w = max_bound_w;
                    let h = (w / aspect).round();
                    if h > max_bound_h {
                        let h = max_bound_h;
                        let w = (h * aspect).round();
                        (w, h)
                    } else {
                        (w, h)
                    }
                } else {
                    let h = max_bound_h;
                    let w = (h * aspect).round();
                    if w > max_bound_w {
                        let w = max_bound_w;
                        let h = (w / aspect).round();
                        (w, h)
                    } else {
                        (w, h)
                    }
                };

                let target_w = (cur_w * scale_factor).round().clamp(min_w, max_w);
                let target_h = (target_w / aspect).round().clamp(min_h, max_h);
                let target_w = (target_h * aspect).round();

                if (cur_w - target_w).abs() >= 1.0 || (cur_h - target_h).abs() >= 1.0 {
                    this.last_window_width = target_w;
                    this.last_window_height = target_h;
                    this.last_kwin_w = target_w;
                    this.last_kwin_h = target_h;
                    window.resize(size(px(target_w), px(target_h)));
                }
            }))
            // Video / Mirror Content Layer (fills 100% of the window since window is always at exact aspect ratio)
            .child(
                div()
                    .size_full()
                    .opacity(if is_hovered { 0.35 } else { 1.0 })
                    .child(if let Some(ref img_data) = self.current_image {
                        gpui::img(img_data.clone())
                            .size_full()
                            .into_any_element()
                    } else {
                        div()
                            .size_full()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap_1()
                            .text_color(rgb(0x4b5563))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child("Connecting to window..."),
                            )
                            .into_any_element()
                    }),
            )
            // Native compositor resize handles on all borders and corners (tracks globally without boundary detachment)
            // Left edge
            .child(
                div()
                    .id("resize-handle-left")
                    .absolute()
                    .top(px(16.0))
                    .bottom(px(16.0))
                    .left_0()
                    .w(px(10.0))
                    .cursor_col_resize()
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                        cx.stop_propagation();
                        window.start_window_resize(ResizeEdge::Left);
                    }),
            )
            // Right edge
            .child(
                div()
                    .id("resize-handle-right")
                    .absolute()
                    .top(px(16.0))
                    .bottom(px(16.0))
                    .right_0()
                    .w(px(10.0))
                    .cursor_col_resize()
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                        cx.stop_propagation();
                        window.start_window_resize(ResizeEdge::Right);
                    }),
            )
            // Top edge
            .child(
                div()
                    .id("resize-handle-top")
                    .absolute()
                    .top_0()
                    .left(px(16.0))
                    .right(px(16.0))
                    .h(px(10.0))
                    .cursor_row_resize()
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                        cx.stop_propagation();
                        window.start_window_resize(ResizeEdge::Top);
                    }),
            )
            // Bottom edge
            .child(
                div()
                    .id("resize-handle-bottom")
                    .absolute()
                    .bottom_0()
                    .left(px(16.0))
                    .right(px(16.0))
                    .h(px(10.0))
                    .cursor_row_resize()
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                        cx.stop_propagation();
                        window.start_window_resize(ResizeEdge::Bottom);
                    }),
            )
            // Top-left corner
            .child(
                div()
                    .id("resize-handle-top-left")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size(px(20.0))
                    .cursor_nwse_resize()
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                        cx.stop_propagation();
                        window.start_window_resize(ResizeEdge::TopLeft);
                    }),
            )
            // Top-right corner
            .child(
                div()
                    .id("resize-handle-top-right")
                    .absolute()
                    .top_0()
                    .right_0()
                    .size(px(20.0))
                    .cursor_nesw_resize()
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                        cx.stop_propagation();
                        window.start_window_resize(ResizeEdge::TopRight);
                    }),
            )
            // Bottom-left corner
            .child(
                div()
                    .id("resize-handle-bottom-left")
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .size(px(20.0))
                    .cursor_nesw_resize()
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                        cx.stop_propagation();
                        window.start_window_resize(ResizeEdge::BottomLeft);
                    }),
            )
            // Bottom-right corner (prominent tactile grip on hover)
            .child(
                div()
                    .id("resize-handle-bottom-right")
                    .absolute()
                    .bottom_0()
                    .right_0()
                    .size(px(24.0))
                    .cursor_nwse_resize()
                    .flex()
                    .items_end()
                    .justify_end()
                    .p_1p5()
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                        cx.stop_propagation();
                        window.start_window_resize(ResizeEdge::BottomRight);
                    })
                    .child(
                        div()
                            .size(px(10.0))
                            .opacity(if is_hovered { 0.75 } else { 0.0 })
                            .flex()
                            .flex_col()
                            .items_end()
                            .justify_end()
                            .gap_0p5()
                            .child(
                                div().flex().gap_0p5().child(
                                    div().size(px(2.0)).rounded_full().bg(rgb(0x4b5563)),
                                ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_0p5()
                                    .child(div().size(px(2.0)).rounded_full().bg(rgb(0x4b5563)))
                                    .child(div().size(px(2.0)).rounded_full().bg(rgb(0x4b5563))),
                            ),
                    ),
            )
            // Subtle stream metadata badge (Bottom-left)
            .child(
                div()
                    .absolute()
                    .bottom_2()
                    .left_2()
                    .px_2()
                    .py_1()
                    .rounded(px(6.0))
                    .bg(rgba(0x00000088))
                    .text_xs()
                    .text_color(rgb(0xf3f4f6))
                    .opacity(if is_hovered { 0.95 } else { 0.5 })
                    .child(format!(
                        "{}×{} • {:.0} fps",
                        self.source_width, self.source_height, self.fps
                    )),
            )
            // Frosty Glass Controls Overlay (Placed ON TOP of resize handles and video)
            .child(
                div()
                    .absolute()
                    .top_2()
                    .right_2()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .opacity(if is_hovered { 1.0 } else { 0.0 })
                    // Minimize button
                    .child(
                        div()
                            .id("btn-minimize")
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(26.0))
                            .rounded(px(13.0))
                            .bg(rgba(0xffffffea))
                            .border_1()
                            .border_color(rgba(0x00000018))
                            .shadow_sm()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgba(0xffffffff)).shadow_md())
                            // Stop propagation so parent's start_window_move() doesn't intercept the click
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.is_minimized = true;
                                this.capture.pause();
                                window.resize(size(px(180.0), px(36.0)));
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .w(px(10.0))
                                    .h(px(2.0))
                                    .bg(rgb(0x374151)),
                            ),
                    )
                    // Close button
                    .child(
                        div()
                            .id("btn-close")
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(26.0))
                            .rounded(px(13.0))
                            .bg(rgba(0xffffffea))
                            .border_1()
                            .border_color(rgba(0x00000018))
                            .shadow_sm()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgba(0xfff1f2fa)).border_color(rgba(0xe11d4840)))
                            // Stop propagation so parent's start_window_move() doesn't intercept the click
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_click(|_, _, cx| {
                                cx.quit();
                            })
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(0x374151))
                                    .child("✕"),
                            ),
                    ),
            )
            .into_any_element()
    }
}
