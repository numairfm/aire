use gpui::{
    px, App, AppContext, Bounds, Context, Entity, Point, Size, WeakEntity,
    WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowHandle, WindowKind,
    WindowOptions,
};

use crate::{
    capture::{create_capture_channel, start_window_capture},
    config::AppConfig,
    tray::TrayHandle,
    ui::{MirrorView, SettingsView},
    window::{set_always_on_top, set_window_resize_constraint},
};

pub const APP_ID: &str = "aire";
pub const SETTINGS_APP_ID: &str = "aire-settings";

pub struct MirrorEntry {
    pub handle: WindowHandle<MirrorView>,
    pub entity: WeakEntity<MirrorView>,
}

pub struct AppState {
    pub config: AppConfig,
    pub tray_handle: Option<TrayHandle>,
    pub mirrors: Vec<MirrorEntry>,
    pub settings_window: Option<WindowHandle<SettingsView>>,
    pub is_picking_window: bool,
}

impl AppState {
    pub fn new(config: AppConfig, tray_handle: Option<TrayHandle>) -> Self {
        Self {
            config,
            tray_handle,
            mirrors: Vec::new(),
            settings_window: None,
            is_picking_window: false,
        }
    }

    pub fn prune_closed_mirrors(&mut self) {
        self.mirrors.retain(|entry| entry.entity.upgrade().is_some());
        self.sync_tray();
    }

    pub fn sync_tray(&self) {
        if let Some(ref tray) = self.tray_handle {
            let active_count = self
                .mirrors
                .iter()
                .filter(|m| m.entity.upgrade().is_some())
                .count();
            tray.update_mirror_state(active_count, false);
        }
    }

    pub fn update_config(&mut self, cx: &mut Context<Self>, mutator: impl FnOnce(&mut AppConfig)) {
        let prev_always_on_top = self.config.always_on_top;
        let prev_fps = self.config.target_fps;

        mutator(&mut self.config);
        self.config = self.config.clone().sanitized();
        let _ = self.config.save();

        if self.config.always_on_top != prev_always_on_top {
            let _ = set_always_on_top(APP_ID, self.config.always_on_top);
        }

        if self.config.target_fps != prev_fps {
            let new_fps = self.config.target_fps;
            for entry in &self.mirrors {
                if let Some(mirror) = entry.entity.upgrade() {
                    mirror.update(cx, |view, _| {
                        view.set_target_fps(new_fps);
                    });
                }
            }
        }

        cx.notify();
    }

    pub fn toggle_pause_all(&mut self, cx: &mut Context<Self>) {
        self.prune_closed_mirrors();
        let any_unpaused = self.mirrors.iter().any(|entry| {
            entry
                .entity
                .upgrade()
                .map(|e| !e.read(cx).is_stream_paused())
                .unwrap_or(false)
        });

        for entry in &self.mirrors {
            if let Some(mirror) = entry.entity.upgrade() {
                mirror.update(cx, |view, cx| {
                    view.set_stream_paused(any_unpaused, cx);
                });
            }
        }

        if let Some(ref tray) = self.tray_handle {
            tray.update_mirror_state(self.mirrors.len(), any_unpaused);
        }
        cx.notify();
    }

    pub fn close_all_mirrors(&mut self, cx: &mut Context<Self>) {
        let entries = std::mem::take(&mut self.mirrors);
        for entry in entries {
            if let Some(mirror) = entry.entity.upgrade() {
                mirror.update(cx, |view, cx| {
                    view.cleanup_before_close(None, cx);
                });
            }
            let _ = entry.handle.update(cx, |_, window, _| {
                window.remove_window();
            });
        }
        self.sync_tray();
        cx.notify();
    }

    pub fn spawn_mirror(state_entity: Entity<Self>, cx: &mut App) {
        let already_picking = state_entity.read(cx).is_picking_window;
        if already_picking {
            return;
        }
        let target_fps = state_entity.read(cx).config.target_fps;
        state_entity.update(cx, |state, cx| {
            state.is_picking_window = true;
            cx.notify();
        });

        cx.spawn(async move |cx| {
            let (event_tx, event_rx) = create_capture_channel();
            let (res_tx, res_rx) = async_channel::bounded(1);

            std::thread::spawn(move || {
                let res = start_window_capture(event_tx, target_fps);
                let _ = res_tx.send_blocking(res);
            });

            let capture_res = res_rx.recv().await;

            let _ = state_entity.update(cx, |state, cx| {
                state.is_picking_window = false;
                cx.notify();
            });

            let Ok(Ok(Some((capture, initial_dims)))) = capture_res else {
                return;
            };

            let _ = cx.update(|cx| {
                let (source_w, source_h) = initial_dims.unwrap_or((1920, 1080));
                let aspect_ratio = if source_h > 0 {
                    source_w as f32 / source_h as f32
                } else {
                    16.0 / 9.0
                };

                let spawn_size = state_entity.read(cx).config.default_spawn_size;
                let mirror_count = state_entity.read(cx).mirrors.len();
                let win_w_f32 = spawn_size.base_width(aspect_ratio);
                let win_h_f32 = (win_w_f32 / aspect_ratio).round().max(100.0);
                let win_width = px(win_w_f32);
                let win_height = px(win_h_f32);

                let display_bounds =
                    cx.displays()
                        .first()
                        .map(|d| d.bounds())
                        .unwrap_or_else(|| Bounds {
                            origin: Point::new(px(0.0), px(0.0)),
                            size: Size::new(px(1920.0), px(1080.0)),
                        });

                // Offset slightly when multiple mirrors are spawned so they don't stack identically
                let cascade_offset = px((mirror_count as f32 % 5.0) * 28.0);
                let origin_x = display_bounds.origin.x
                    + (display_bounds.size.width - win_width) / 2.0
                    + cascade_offset;
                let origin_y = display_bounds.origin.y
                    + (display_bounds.size.height - win_height) / 2.0
                    + cascade_offset;

                let (min_bounds, max_bounds) = MirrorView::compute_min_max_bounds(
                    aspect_ratio,
                    f32::from(display_bounds.size.width),
                    f32::from(display_bounds.size.height),
                );
                set_window_resize_constraint(
                    Some(aspect_ratio),
                    Some(min_bounds),
                    Some(max_bounds),
                );

                let options = WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: Point::new(origin_x, origin_y),
                        size: Size::new(win_width, win_height),
                    })),
                    titlebar: None,
                    app_id: Some(APP_ID.to_string()),
                    window_decorations: Some(WindowDecorations::Client),
                    window_background: WindowBackgroundAppearance::Transparent,
                    kind: WindowKind::PopUp,
                    is_movable: true,
                    is_resizable: true,
                    window_min_size: Some(Size::new(px(min_bounds.0), px(min_bounds.1))),
                    ..Default::default()
                };

                let state_for_mirror = state_entity.clone();
                let mut created_entity: Option<WeakEntity<MirrorView>> = None;

                if let Ok(handle) = cx.open_window(options, |_, cx| {
                    cx.activate(false);
                    let view = cx.new(|cx| {
                        MirrorView::new(
                            cx,
                            state_for_mirror,
                            capture,
                            event_rx,
                            aspect_ratio,
                            source_w,
                            source_h,
                        )
                    });
                    created_entity = Some(view.downgrade());
                    view
                }) {
                    if let Some(weak_view) = created_entity {
                        state_entity.update(cx, |state, cx| {
                            state.prune_closed_mirrors();
                            state.mirrors.push(MirrorEntry {
                                handle,
                                entity: weak_view,
                            });
                            state.sync_tray();
                            cx.notify();
                        });
                    }
                }
            });
        })
        .detach();
    }

    pub fn open_settings(state_entity: Entity<Self>, cx: &mut App) {
        if let Some(existing) = state_entity.read(cx).settings_window {
            let still_open = existing
                .update(cx, |_, window, _| {
                    window.activate_window();
                })
                .is_ok();
            if still_open {
                return;
            }
        }

        let win_width = px(460.0);
        let win_height = px(520.0);

        let display_bounds =
            cx.displays()
                .first()
                .map(|d| d.bounds())
                .unwrap_or_else(|| Bounds {
                    origin: Point::new(px(0.0), px(0.0)),
                    size: Size::new(px(1920.0), px(1080.0)),
                });

        let origin_x = display_bounds.origin.x + (display_bounds.size.width - win_width) / 2.0;
        let origin_y = display_bounds.origin.y + (display_bounds.size.height - win_height) / 2.0;

        // Lock the Settings window to its exact 460x520 size
        set_window_resize_constraint(None, Some((460.0, 520.0)), Some((460.0, 520.0)));

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::new(origin_x, origin_y),
                size: Size::new(win_width, win_height),
            })),
            titlebar: None,
            app_id: Some(SETTINGS_APP_ID.to_string()),
            window_decorations: Some(WindowDecorations::Client),
            window_background: WindowBackgroundAppearance::Transparent,
            kind: WindowKind::Normal,
            is_movable: true,
            is_resizable: false,
            window_min_size: Some(Size::new(win_width, win_height)),
            ..Default::default()
        };

        let state_for_view = state_entity.clone();
        if let Ok(handle) = cx.open_window(options, |_, cx| {
            cx.activate(true);
            cx.new(|cx| SettingsView::new(cx, state_for_view))
        }) {
            state_entity.update(cx, |state, _| {
                state.settings_window = Some(handle);
            });
        }
    }
}
