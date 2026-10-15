use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use crossbeam_channel::Receiver;
use gpui::{
    div, prelude::*, px, rgb, rgba, size, Context, FontWeight, Render, RenderImage,
    ResizeEdge, WeakEntity, Window,
};

use crate::capture::{CaptureEvent, WindowCapture};

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
}

impl MirrorView {
    pub fn new(
        cx: &mut Context<Self>,
        capture: Box<dyn WindowCapture>,
        event_rx: Receiver<CaptureEvent>,
    ) -> Self {
        // Spawn background listener for capture events
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let mut frames_this_sec = 0;
            let mut last_fps_time = Instant::now();

            loop {
                match event_rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(event) => match event {
                        CaptureEvent::Frame(frame) => {
                            frames_this_sec += 1;
                            let now = Instant::now();
                            let mut current_fps = 0.0;
                            if now.duration_since(last_fps_time) >= Duration::from_secs(1) {
                                current_fps = frames_this_sec as f64
                                    / now.duration_since(last_fps_time).as_secs_f64();
                                frames_this_sec = 0;
                                last_fps_time = now;
                            }

                            let _ = this.update(cx, |view, cx| {
                                view.current_image = Some(frame.render_image);
                                view.source_width = frame.width;
                                view.source_height = frame.height;
                                if frame.height > 0 {
                                    view.aspect_ratio = frame.width as f32 / frame.height as f32;
                                }
                                if current_fps > 0.0 {
                                    view.fps = current_fps;
                                }
                                cx.notify();
                            });
                        }
                        CaptureEvent::Resized { width, height } => {
                            let _ = this.update(cx, |view, cx| {
                                view.source_width = width;
                                view.source_height = height;
                                if height > 0 {
                                    view.aspect_ratio = width as f32 / height as f32;
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
                    },
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                        // Yield to executor when no new frames are coming in (adaptive)
                        cx.background_executor().timer(Duration::from_millis(16)).await;
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                        let _ = this.update(cx, |view, cx| {
                            view.source_closed = true;
                            cx.notify();
                        });
                        break;
                    }
                }
            }
        })
        .detach();

        Self {
            capture,
            current_image: None,
            source_width: 1920,
            source_height: 1080,
            aspect_ratio: 16.0 / 9.0,
            fps: 0.0,
            is_minimized: false,
            source_closed: false,
            error_message: None,
            last_window_width: 400.0,
            last_window_height: 225.0,
        }
    }
}

impl Render for MirrorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Track current window size to restore after un-minimizing
        if !self.is_minimized {
            let bounds = window.bounds();
            self.last_window_width = f32::from(bounds.size.width);
            self.last_window_height = f32::from(bounds.size.height);
        }

        // 1. Minimized Pill State
        if self.is_minimized {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                // Light frosty pill glass style
                .bg(rgba(0xffffffea))
                .rounded(px(18.0))
                .border_1()
                .border_color(rgba(0xffffffbb))
                .shadow_lg()
                .cursor_pointer()
                .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                    window.start_window_move();
                })
                .id("restore-pill")
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
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .child(
                            div()
                                .size(px(8.0))
                                .rounded(px(4.0))
                                .bg(rgb(0x10b981)), // gentle emerald green pulse
                        )
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(0x1f2937))
                                .child("aire • click to restore"),
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
                        .on_click(|_, _, cx| {
                            cx.quit();
                        })
                        .child("Exit"),
                )
                .into_any_element();
        }

        // 4. Normal Active Mirror HUD
        div()
            .group("hud")
            .size_full()
            .relative()
            // Frosty light glass styling
            .bg(rgba(0xffffff28))
            .rounded(px(10.0))
            .border_1()
            .border_color(rgba(0xffffff50))
            .shadow_xl()
            .overflow_hidden()
            // Click-drag anywhere to move window
            .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                window.start_window_move();
            })
            // Video / Mirror Content Layer (eases to 35% opacity on hover)
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
            // Frosty Glass Controls Overlay (Fades in on hover)
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
                            .size(px(26.0))
                            .rounded(px(13.0))
                            .bg(rgba(0xffffffea))
                            .border_1()
                            .border_color(rgba(0x00000018))
                            .shadow_sm()
                            .cursor_pointer()
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
                    .opacity(0.5)
                    .group_hover("hud", |s| s.opacity(0.95))
                    .child(format!(
                        "{}×{} • {:.0} fps",
                        self.source_width, self.source_height, self.fps
                    )),
            )
            // Resize handles on borders
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
            // Bottom-left corner
            .child(
                div()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .size(px(14.0))
                    .cursor_nesw_resize()
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                        window.start_window_resize(ResizeEdge::BottomLeft);
                    }),
            )
            // Top-right corner
            .child(
                div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .size(px(14.0))
                    .cursor_nesw_resize()
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                        window.start_window_resize(ResizeEdge::TopRight);
                    }),
            )
            // Top-left corner
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size(px(14.0))
                    .cursor_nwse_resize()
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                        window.start_window_resize(ResizeEdge::TopLeft);
                    }),
            )
            .into_any_element()
    }
}
