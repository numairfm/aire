use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{
    canvas, div, point, prelude::*, px, rgb, size, Context, Entity, FontWeight, PathBuilder,
    Render, RenderImage, ResizeEdge, ScrollWheelEvent, Task, WeakEntity, Window,
};

use crate::{
    capture::{
        create_capture_channel, start_window_capture, CaptureEvent, CaptureReceiver, WindowCapture,
    },
    state::AppState,
    window::{remove_window_resize_constraint, set_window_resize_constraint_for},
};

pub struct MirrorView {
    app_state: Entity<AppState>,
    capture: Box<dyn WindowCapture>,
    current_image: Option<Arc<RenderImage>>,
    retired_images: Vec<Arc<RenderImage>>,
    source_width: u32,
    source_height: u32,
    aspect_ratio: f32,
    fps: f64,
    is_minimized: bool,
    is_paused_by_user: bool,
    is_switching_source: bool,
    source_closed: bool,
    error_message: Option<String>,
    last_window_width: f32,
    last_window_height: f32,
    needs_aspect_adaptation: bool,
    was_hovered: bool,
    hover_transition_start: Option<Instant>,
    hover_transition_from: f32,
    current_hover_t: f32,
    _receiver_task: Task<()>,
}

impl MirrorView {
    pub fn compute_min_max_bounds(
        aspect_ratio: f32,
        disp_w: f32,
        disp_h: f32,
    ) -> ((f32, f32), (f32, f32)) {
        let aspect = if aspect_ratio > 0.05 {
            aspect_ratio
        } else {
            16.0 / 9.0
        };

        let (min_w, min_h) = if aspect >= 1.0 {
            let w = 240.0f32;
            let h = (w / aspect).round().max(135.0);
            let w = (h * aspect).round();
            (w, h)
        } else {
            let h = 240.0f32;
            let w = (h * aspect).round().max(160.0);
            let h = (w / aspect).round();
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

        ((min_w, min_h), (max_w, max_h))
    }

    pub fn new(
        cx: &mut Context<Self>,
        app_state: Entity<AppState>,
        capture: Box<dyn WindowCapture>,
        event_rx: CaptureReceiver,
        initial_aspect_ratio: f32,
        initial_width: u32,
        initial_height: u32,
    ) -> Self {
        cx.observe(&app_state, |_, _, cx| {
            cx.notify();
        })
        .detach();

        let state_for_release = app_state.clone();
        cx.on_release(move |view, cx| {
            view.capture.stop();
            state_for_release.update(cx, |state, _| {
                state.prune_closed_mirrors();
            });
        })
        .detach();

        let receiver_task = Self::spawn_receiver_task(cx, event_rx);

        Self {
            app_state,
            capture,
            current_image: None,
            retired_images: Vec::with_capacity(4),
            source_width: initial_width,
            source_height: initial_height,
            aspect_ratio: initial_aspect_ratio,
            fps: 0.0,
            is_minimized: false,
            is_paused_by_user: false,
            is_switching_source: false,
            source_closed: false,
            error_message: None,
            last_window_width: 0.0,
            last_window_height: 0.0,
            needs_aspect_adaptation: false,
            was_hovered: false,
            hover_transition_start: None,
            hover_transition_from: 0.0,
            current_hover_t: 0.0,
            _receiver_task: receiver_task,
        }
    }

    fn spawn_receiver_task(cx: &mut Context<Self>, event_rx: CaptureReceiver) -> Task<()> {
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let mut frames_this_sec = 0;
            let mut last_fps_time = Instant::now();
            let last_frame_instant = Arc::new(parking_lot::Mutex::new(Instant::now()));

            let this_fps_watcher = this.clone();
            let last_frame_instant_checker = last_frame_instant.clone();
            let _fps_checker = cx.spawn(async move |cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(500))
                        .await;
                    let last_seen = *last_frame_instant_checker.lock();
                    let ok = this_fps_watcher.update(cx, |view, cx| {
                        if view.fps > 0.0 && last_seen.elapsed() > Duration::from_millis(1500) {
                            view.fps = 0.0;
                            cx.notify();
                        }
                    });
                    if ok.is_err() {
                        break;
                    }
                }
            });

            while let Ok(event) = event_rx.recv().await {
                match event {
                    CaptureEvent::Frame(mut frame) => {
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
                                        if let Some(old_img) = view.current_image.take() {
                                            cx.drop_image(old_img.clone(), None);
                                            view.retired_images.push(old_img);
                                        }
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

                        let update_res = this.update(cx, |view, cx| {
                            if let Some(old_img) = view.current_image.replace(frame.render_image) {
                                cx.drop_image(old_img.clone(), None);
                                if view.retired_images.len() >= 2 {
                                    view.retired_images.remove(0);
                                }
                                view.retired_images.push(old_img);
                            }
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
                        if update_res.is_err() {
                            break;
                        }
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
                            if let Some(old_img) = view.current_image.take() {
                                cx.drop_image(old_img.clone(), None);
                                view.retired_images.push(old_img);
                            }
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
    }

    pub fn set_target_fps(&mut self, fps: u32) {
        self.capture.set_target_fps(fps);
    }

    pub fn is_stream_paused(&self) -> bool {
        self.is_paused_by_user || self.is_minimized
    }

    pub fn set_stream_paused(&mut self, paused: bool, cx: &mut Context<Self>) {
        self.is_paused_by_user = paused;
        if paused {
            self.capture.pause();
            self.fps = 0.0;
        } else if !self.is_minimized {
            self.capture.resume();
        }
        cx.notify();
    }

    pub fn cleanup_before_close(&mut self, window: Option<&mut Window>, cx: &mut Context<Self>) {
        self.capture.stop();
        if let Some(old_img) = self.current_image.take() {
            self.retired_images.push(old_img);
        }
        if let Some(win) = window {
            let win_id = win.window_handle().window_id().as_u64();
            remove_window_resize_constraint(win_id);
            for img in self.retired_images.drain(..) {
                cx.drop_image(img, Some(win));
            }
        } else {
            for img in self.retired_images.drain(..) {
                cx.drop_image(img, None);
            }
        }
    }

    fn close_this_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cleanup_before_close(Some(window), cx);
        let app_state = self.app_state.clone();
        window.remove_window();
        app_state.update(cx, |state, cx| {
            state.prune_closed_mirrors();
            cx.notify();
        });
    }

    fn trigger_switch_source(&mut self, cx: &mut Context<Self>) {
        if self.is_switching_source || self.app_state.read(cx).is_picking_window {
            return;
        }
        self.is_switching_source = true;
        let target_fps = self.app_state.read(cx).config.target_fps;
        let app_state_entity = self.app_state.clone();
        app_state_entity.update(cx, |state, cx| {
            state.is_picking_window = true;
            cx.notify();
        });
        cx.notify();

        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let (event_tx, event_rx) = create_capture_channel();
            let (res_tx, res_rx) = async_channel::bounded(1);

            std::thread::spawn(move || {
                let res = start_window_capture(event_tx, target_fps);
                let _ = res_tx.send_blocking(res);
            });

            let capture_res = res_rx.recv().await;

            let _ = app_state_entity.update(cx, |state, cx| {
                state.is_picking_window = false;
                cx.notify();
            });

            let _ = this.update(cx, |view, cx| {
                view.is_switching_source = false;
                if let Ok(Ok(Some((new_capture, initial_dims)))) = capture_res {
                    view.capture.stop();
                    if let Some(old_img) = view.current_image.take() {
                        cx.drop_image(old_img.clone(), None);
                        view.retired_images.push(old_img);
                    }
                    view.capture = new_capture;
                    view.source_closed = false;
                    view.error_message = None;
                    view.is_paused_by_user = false;
                    if let Some((w, h)) = initial_dims {
                        view.source_width = w;
                        view.source_height = h;
                        if h > 0 {
                            view.aspect_ratio = w as f32 / h as f32;
                            view.needs_aspect_adaptation = true;
                        }
                    }
                    view._receiver_task = Self::spawn_receiver_task(cx, event_rx);
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Drop for MirrorView {
    fn drop(&mut self) {
        self.capture.stop();
    }
}

impl Render for MirrorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let win_id = window.window_handle().window_id().as_u64();

        // Evict all retired frame textures from this window's WGPU sprite atlas before painting
        for old_img in self.retired_images.drain(..) {
            cx.drop_image(old_img, Some(window));
        }

        let config = self.app_state.read(cx).config.clone();

        // 1. Sync Wayland protocol-level aspect ratio & boundary constraints for THIS window
        if !self.is_minimized {
            let bounds = window.bounds();
            let raw_w = f32::from(bounds.size.width);
            let raw_h = f32::from(bounds.size.height);

            let aspect = if self.aspect_ratio > 0.05 {
                self.aspect_ratio
            } else {
                16.0 / 9.0
            };

            let (disp_w, disp_h) = window
                .display(cx)
                .map(|d| {
                    (
                        f32::from(d.bounds().size.width),
                        f32::from(d.bounds().size.height),
                    )
                })
                .unwrap_or((1920.0, 1080.0));

            let ((min_w, min_h), (max_w, max_h)) =
                Self::compute_min_max_bounds(aspect, disp_w, disp_h);

            set_window_resize_constraint_for(
                Some(win_id),
                Some(aspect),
                Some((min_w, min_h)),
                Some((max_w, max_h)),
            );

            if self.needs_aspect_adaptation {
                self.needs_aspect_adaptation = false;
                let w = raw_w.clamp(min_w, max_w);
                let h = (w / aspect).round().clamp(min_h, max_h);
                let w = (h * aspect).round().clamp(min_w, max_w);
                self.last_window_width = w;
                self.last_window_height = h;
                if (raw_w - w).abs() >= 1.0 || (raw_h - h).abs() >= 1.0 {
                    window.resize(size(px(w), px(h)));
                }
            } else {
                self.last_window_width = raw_w;
                self.last_window_height = raw_h;
            }
        }

        // 2. Minimized Pill State (Opaque Black + Monochrome Dark Border)
        if self.is_minimized {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_between()
                .px_3()
                .bg(rgb(0x000000))
                .rounded(px(18.0))
                .border_3()
                .border_color(rgb(0x262626))
                .overflow_hidden()
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
                                .bg(rgb(0xffffff)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(0xffffff))
                                .child("aire"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x737373))
                                .child("⋮"),
                        ),
                )
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
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.is_minimized = false;
                            this.was_hovered = false;
                            this.current_hover_t = 0.0;
                            this.hover_transition_start = None;
                            if !this.is_paused_by_user {
                                this.capture.resume();
                            }
                            let (disp_w, disp_h) = window
                                .display(cx)
                                .map(|d| {
                                    (
                                        f32::from(d.bounds().size.width),
                                        f32::from(d.bounds().size.height),
                                    )
                                })
                                .unwrap_or((1920.0, 1080.0));
                            let aspect = if this.aspect_ratio > 0.05 {
                                this.aspect_ratio
                            } else {
                                16.0 / 9.0
                            };
                            let (min_bounds, max_bounds) =
                                Self::compute_min_max_bounds(aspect, disp_w, disp_h);
                            set_window_resize_constraint_for(
                                Some(win_id),
                                Some(aspect),
                                Some(min_bounds),
                                Some(max_bounds),
                            );
                            window.resize(size(
                                px(this.last_window_width.max(min_bounds.0)),
                                px(this.last_window_height.max(min_bounds.1)),
                            ));
                            cx.notify();
                        }))
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(0xa3a3a3))
                                .hover(|s| s.text_color(rgb(0xffffff)))
                                .child("restore ↗"),
                        ),
                )
                .into_any_element();
        }

        // 3. Source Window Closed State (Opaque Black + Monochrome Border)
        if self.source_closed {
            return div()
                .size_full()
                .relative()
                .bg(rgb(0x000000))
                .rounded(px(14.0))
                .border_4()
                .border_color(rgb(0x222222))
                .overflow_hidden()
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
                        .text_color(rgb(0xffffff))
                        .child("Source window closed"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0xa3a3a3))
                        .child("The mirrored window was closed."),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .mt_2()
                        .child(
                            div()
                                .id("btn-switch-closed")
                                .px_3()
                                .py_1p5()
                                .rounded(px(8.0))
                                .bg(rgb(0xffffff))
                                .text_xs()
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(0x000000))
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(0xe5e5e5)))
                                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation();
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.trigger_switch_source(cx);
                                }))
                                .child("Switch Window"),
                        )
                        .child(
                            div()
                                .id("btn-exit-closed")
                                .px_3()
                                .py_1p5()
                                .rounded(px(8.0))
                                .bg(rgb(0x1a1a1a))
                                .border_1()
                                .border_color(rgb(0x2c2c2c))
                                .text_xs()
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(0xffffff))
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(0x262626)).border_color(rgb(0x3a3a3a)))
                                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation();
                                })
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_this_window(window, cx);
                                }))
                                .child("Close Mirror"),
                        ),
                )
                .into_any_element();
        }

        // 4. Error State (Opaque Black + Monochrome Border)
        if let Some(ref err) = self.error_message {
            return div()
                .size_full()
                .relative()
                .bg(rgb(0x000000))
                .rounded(px(14.0))
                .border_4()
                .border_color(rgb(0x222222))
                .overflow_hidden()
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
                        .text_color(rgb(0xffffff))
                        .child("Capture could not start"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(0xa3a3a3))
                        .child(err.clone()),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .mt_2()
                        .child(
                            div()
                                .id("btn-switch-error")
                                .px_3()
                                .py_1p5()
                                .rounded(px(8.0))
                                .bg(rgb(0xffffff))
                                .text_xs()
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(0x000000))
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(0xe5e5e5)))
                                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation();
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.trigger_switch_source(cx);
                                }))
                                .child("Switch Window"),
                        )
                        .child(
                            div()
                                .id("btn-exit-error")
                                .px_3()
                                .py_1p5()
                                .rounded(px(8.0))
                                .bg(rgb(0x1a1a1a))
                                .border_1()
                                .border_color(rgb(0x2c2c2c))
                                .text_xs()
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(0xffffff))
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(0x262626)).border_color(rgb(0x3a3a3a)))
                                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation();
                                })
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_this_window(window, cx);
                                }))
                                .child("Close Mirror"),
                        ),
                )
                .into_any_element();
        }

        // 5. Smooth Hover Animation Calculation
        let is_hovered = window.is_window_hovered();
        let target_t = if is_hovered { 1.0f32 } else { 0.0f32 };
        let anim_ms = config.animation_speed.duration_ms();

        if is_hovered != self.was_hovered {
            self.was_hovered = is_hovered;
            if anim_ms == 0 {
                self.current_hover_t = target_t;
                self.hover_transition_start = None;
            } else {
                self.hover_transition_from = self.current_hover_t;
                self.hover_transition_start = Some(Instant::now());
            }
        }

        if let Some(start) = self.hover_transition_start {
            let elapsed_ms = start.elapsed().as_secs_f32() * 1000.0;
            let duration_f32 = (anim_ms as f32).max(1.0);
            let progress = (elapsed_ms / duration_f32).clamp(0.0, 1.0);
            let eased = 1.0 - (1.0 - progress).powi(3);
            self.current_hover_t =
                self.hover_transition_from + (target_t - self.hover_transition_from) * eased;
            if progress < 1.0 {
                window.request_animation_frame();
            } else {
                self.current_hover_t = target_t;
                self.hover_transition_start = None;
            }
        } else {
            self.current_hover_t = target_t;
        }

        let stream_opacity =
            1.0 - self.current_hover_t * (1.0 - config.hover_opacity.clamp(0.05, 1.0));
        let hud_opacity = self.current_hover_t;

        // Note: No outer box-shadow on the root full-surface div so KWin Wayland never leaves
        // bottom-border shadow bleed trails when moving the window across the desktop.
        div()
            .id("mirror-root")
            .size_full()
            .relative()
            .bg(rgb(0x000000))
            .rounded(px(14.0))
            .border_4()
            .border_color(rgb(0x222222))
            .overflow_hidden()
            .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                window.start_window_move();
            })
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

                let ((min_w, min_h), (max_w, max_h)) =
                    Self::compute_min_max_bounds(aspect, disp_w, disp_h);

                let target_w = (cur_w * scale_factor).round().clamp(min_w, max_w);
                let target_h = (target_w / aspect).round().clamp(min_h, max_h);
                let target_w = (target_h * aspect).round().clamp(min_w, max_w);

                if (cur_w - target_w).abs() >= 1.0 || (cur_h - target_h).abs() >= 1.0 {
                    this.last_window_width = target_w;
                    this.last_window_height = target_h;
                    window.resize(size(px(target_w), px(target_h)));
                }
            }))
            // Video / Mirror Content Layer
            .child(
                div()
                    .size_full()
                    .opacity(stream_opacity)
                    .child(if let Some(ref img_data) = self.current_image {
                        gpui::img(img_data.clone())
                            .size_full()
                            .into_any_element()
                    } else {
                        div()
                            .size_full()
                            .bg(rgb(0x000000))
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap_1()
                            .text_color(rgb(0xa3a3a3))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Connecting to window..."),
                            )
                            .into_any_element()
                    }),
            )
            // Native compositor resize handles on all borders and corners
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
                            .opacity(hud_opacity * 0.85)
                            .flex()
                            .flex_col()
                            .items_end()
                            .justify_end()
                            .gap_0p5()
                            .child(
                                div().flex().gap_0p5().child(
                                    div().size(px(2.5)).rounded_full().bg(rgb(0xf9fafb)),
                                ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_0p5()
                                    .child(div().size(px(2.5)).rounded_full().bg(rgb(0xf9fafb)))
                                    .child(div().size(px(2.5)).rounded_full().bg(rgb(0xf9fafb))),
                            ),
                    ),
            )
            // Optional Debug FPS & Resolution Badge
            .when(config.show_fps_badge, |el| {
                el.child(
                    div()
                        .absolute()
                        .bottom_2p5()
                        .left_2p5()
                        .px_2p5()
                        .py_1()
                        .rounded(px(8.0))
                        .bg(rgb(0x000000))
                        .border_1()
                        .border_color(rgb(0x2c2c2c))
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0xffffff))
                        .opacity(if is_hovered { 0.98 } else { 0.65 })
                        .child(format!(
                            "{}×{} • {:.0} fps{}",
                            self.source_width,
                            self.source_height,
                            self.fps,
                            if self.is_paused_by_user { " (paused)" } else { "" }
                        )),
                )
            })
            // Monochrome Opaque Window Controls Overlay (Top-Right: Switch Source, Minimize, Close)
            .child(
                div()
                    .absolute()
                    .top_2p5()
                    .right_2p5()
                    .flex()
                    .items_center()
                    .gap_2()
                    .opacity(hud_opacity)
                    // Switch Source button (`⇄`) — Opaque monochrome pill with precision vector canvas icon
                    .child(
                        div()
                            .id("btn-switch-source")
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(30.0))
                            .rounded(px(15.0))
                            .bg(rgb(0x000000))
                            .border_1()
                            .border_color(rgb(0x333333))
                            .shadow_sm()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x1f1f1f)).border_color(rgb(0x555555)))
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.trigger_switch_source(cx);
                            }))
                            .child(
                                canvas(
                                    |_, _, _| {},
                                    move |bounds, _, window, _| {
                                        let cx = bounds.origin.x + bounds.size.width / 2.0;
                                        let cy = bounds.origin.y + bounds.size.height / 2.0;

                                        // Top arrow (pointing right)
                                        let mut b1 = PathBuilder::stroke(px(1.8));
                                        b1.move_to(point(cx - px(5.5), cy - px(2.5)));
                                        b1.line_to(point(cx + px(5.0), cy - px(2.5)));
                                        b1.move_to(point(cx + px(2.2), cy - px(5.5)));
                                        b1.line_to(point(cx + px(5.0), cy - px(2.5)));
                                        b1.line_to(point(cx + px(2.2), cy + px(0.5)));
                                        if let Ok(path) = b1.build() {
                                            window.paint_path(path, rgb(0xffffff));
                                        }

                                        // Bottom arrow (pointing left)
                                        let mut b2 = PathBuilder::stroke(px(1.8));
                                        b2.move_to(point(cx + px(5.5), cy + px(2.5)));
                                        b2.line_to(point(cx - px(5.0), cy + px(2.5)));
                                        b2.move_to(point(cx - px(2.2), cy - px(0.5)));
                                        b2.line_to(point(cx - px(5.0), cy + px(2.5)));
                                        b2.line_to(point(cx - px(2.2), cy + px(5.5)));
                                        if let Ok(path) = b2.build() {
                                            window.paint_path(path, rgb(0xffffff));
                                        }
                                    },
                                )
                                .size(px(15.0)),
                            ),
                    )
                    // Minimize to pill button (`—`) — Opaque monochrome pill with precision vector canvas bar
                    .child(
                        div()
                            .id("btn-minimize")
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(30.0))
                            .rounded(px(15.0))
                            .bg(rgb(0x000000))
                            .border_1()
                            .border_color(rgb(0x333333))
                            .shadow_sm()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x1f1f1f)).border_color(rgb(0x555555)))
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let cur_bounds = window.bounds();
                                let w_f32 = f32::from(cur_bounds.size.width);
                                let h_f32 = f32::from(cur_bounds.size.height);
                                if w_f32 > 190.0 && h_f32 > 45.0 {
                                    this.last_window_width = w_f32;
                                    this.last_window_height = h_f32;
                                }
                                this.is_minimized = true;
                                this.was_hovered = false;
                                this.current_hover_t = 0.0;
                                this.hover_transition_start = None;
                                this.capture.pause();
                                set_window_resize_constraint_for(
                                    Some(win_id),
                                    None,
                                    Some((180.0, 36.0)),
                                    Some((180.0, 36.0)),
                                );
                                window.resize(size(px(180.0), px(36.0)));
                                cx.notify();
                            }))
                            .child(
                                canvas(
                                    |_, _, _| {},
                                    move |bounds, _, window, _| {
                                        let cx = bounds.origin.x + bounds.size.width / 2.0;
                                        let cy = bounds.origin.y + bounds.size.height / 2.0;
                                        let mut b = PathBuilder::stroke(px(2.2));
                                        b.move_to(point(cx - px(5.5), cy));
                                        b.line_to(point(cx + px(5.5), cy));
                                        if let Ok(path) = b.build() {
                                             window.paint_path(path, rgb(0xffffff));
                                        }
                                    },
                                )
                                .size(px(14.0)),
                            ),
                    )
                    // Close mirror button (`✕`) — Opaque monochrome pill with precision vector canvas cross
                    .child(
                        div()
                            .id("btn-close")
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(30.0))
                            .rounded(px(15.0))
                            .bg(rgb(0x000000))
                            .border_1()
                            .border_color(rgb(0x333333))
                            .shadow_sm()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0x1f1f1f)).border_color(rgb(0x555555)))
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_this_window(window, cx);
                            }))
                            .child(
                                canvas(
                                    |_, _, _| {},
                                    move |bounds, _, window, _| {
                                        let cx = bounds.origin.x + bounds.size.width / 2.0;
                                        let cy = bounds.origin.y + bounds.size.height / 2.0;
                                        let r = px(4.5);
                                        let mut b1 = PathBuilder::stroke(px(2.2));
                                        b1.move_to(point(cx - r, cy - r));
                                        b1.line_to(point(cx + r, cy + r));
                                        if let Ok(path) = b1.build() {
                                            window.paint_path(path, rgb(0xffffff));
                                        }
                                        let mut b2 = PathBuilder::stroke(px(2.2));
                                        b2.move_to(point(cx + r, cy - r));
                                        b2.line_to(point(cx - r, cy + r));
                                        if let Ok(path) = b2.build() {
                                            window.paint_path(path, rgb(0xffffff));
                                        }
                                    },
                                )
                                .size(px(14.0)),
                            ),
                    ),
            )
            .into_any_element()
    }
}
