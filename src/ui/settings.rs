use gpui::{
    canvas, div, point, prelude::*, px, rgb, Context, Entity, FontWeight, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PathBuilder, Render, Window,
};

use crate::{
    config::{AnimationSpeed, SpawnSize},
    state::AppState,
    window::{remove_window_resize_constraint, set_window_resize_constraint_for},
};

pub struct SettingsView {
    app_state: Entity<AppState>,
    is_dragging_slider: bool,
}

impl SettingsView {
    pub fn new(cx: &mut Context<Self>, app_state: Entity<AppState>) -> Self {
        cx.observe(&app_state, |_, _, cx| {
            cx.notify();
        })
        .detach();

        let state_for_release = app_state.clone();
        cx.on_release(move |_, cx| {
            state_for_release.update(cx, |state, _| {
                state.settings_window = None;
            });
        })
        .detach();

        Self {
            app_state,
            is_dragging_slider: false,
        }
    }

    fn update_slider_from_x(&mut self, mouse_x: f32, window_width: f32, cx: &mut Context<Self>) {
        let track_left = 37.0f32;
        let track_right = (window_width - 37.0).max(track_left + 100.0);
        let t = ((mouse_x - track_left) / (track_right - track_left)).clamp(0.0, 1.0);
        let opacity = (0.05 + t * 0.95).clamp(0.05, 1.0);
        let rounded = (opacity * 100.0).round() / 100.0;

        self.app_state.update(cx, |state, cx| {
            state.update_config(cx, |cfg| {
                cfg.hover_opacity = rounded;
            });
        });
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let win_id = window.window_handle().window_id().as_u64();
        set_window_resize_constraint_for(
            Some(win_id),
            None,
            Some((460.0, 580.0)),
            Some((460.0, 580.0)),
        );

        let win_w = f32::from(window.bounds().size.width).max(460.0);
        let (config, active_mirrors, is_picking, all_paused) = {
            let state = self.app_state.read(cx);
            let count = state
                .mirrors
                .iter()
                .filter(|m| m.entity.upgrade().is_some())
                .count();
            let all_paused = count > 0
                && state
                    .mirrors
                    .iter()
                    .filter_map(|m| m.entity.upgrade())
                    .all(|e| e.read(cx).is_stream_paused());
            (state.config.clone(), count, state.is_picking_window, all_paused)
        };

        let opacity_pct = (config.hover_opacity * 100.0).round() as u32;
        let slider_ratio = ((config.hover_opacity - 0.05) / 0.95).clamp(0.0, 1.0);
        let track_width = (win_w - 74.0).max(200.0);
        let fill_width = (track_width * slider_ratio).round();

        div()
            .id("settings-root")
            .size_full()
            .bg(rgb(0x000000))
            .rounded(px(16.0))
            .border_1()
            .border_color(rgb(0x333333))
            .flex()
            .flex_col()
            .overflow_hidden()
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                if this.is_dragging_slider {
                    if event.pressed_button == Some(gpui::MouseButton::Left) {
                        this.update_slider_from_x(f32::from(event.position.x), win_w, cx);
                    } else {
                        this.is_dragging_slider = false;
                        cx.notify();
                    }
                }
            }))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    if this.is_dragging_slider {
                        this.is_dragging_slider = false;
                        cx.notify();
                    }
                }),
            )
            // Top Draggable Opaque Black Header
            .child(
                div()
                    .w_full()
                    .px_5()
                    .py_3p5()
                    .bg(rgb(0x0c0c0c))
                    .border_b_1()
                    .border_color(rgb(0x262626))
                    .flex()
                    .items_center()
                    .justify_between()
                    .cursor_grab()
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                        window.start_window_move();
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .size(px(9.0))
                                    .rounded(px(4.5))
                                    .bg(rgb(0xffffff)),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::EXTRA_BOLD)
                                    .text_color(rgb(0xffffff))
                                    .child("aire"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x737373))
                                    .child("Control Panel"),
                            ),
                    )
                    // Opaque Monochrome Close Button with Vector Canvas Icon
                    .child(
                        div()
                            .id("btn-close-settings")
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(30.0))
                            .rounded(px(15.0))
                            .bg(rgb(0x1a1a1a))
                            .border_1()
                            .border_color(rgb(0x333333))
                            .shadow_sm()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(0xdc2626)).border_color(rgb(0xef4444)))
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                remove_window_resize_constraint(win_id);
                                this.app_state.update(cx, |state, _| {
                                    state.settings_window = None;
                                });
                                window.remove_window();
                            }))
                            .child(
                                canvas(
                                    |_, _, _| {},
                                    move |bounds, _, window, _| {
                                        let center_x = bounds.origin.x + bounds.size.width / 2.0;
                                        let center_y = bounds.origin.y + bounds.size.height / 2.0;
                                        let r = px(4.5);
                                        let mut b1 = PathBuilder::stroke(px(2.2));
                                        b1.move_to(point(center_x - r, center_y - r));
                                        b1.line_to(point(center_x + r, center_y + r));
                                        if let Ok(path) = b1.build() {
                                            window.paint_path(path, rgb(0xffffff));
                                        }
                                        let mut b2 = PathBuilder::stroke(px(2.2));
                                        b2.move_to(point(center_x + r, center_y - r));
                                        b2.line_to(point(center_x - r, center_y + r));
                                        if let Ok(path) = b2.build() {
                                            window.paint_path(path, rgb(0xffffff));
                                        }
                                    },
                                )
                                .size(px(14.0)),
                            ),
                    ),
            )
            // Body Content
            .child(
                div()
                    .flex_1()
                    .px_5()
                    .py_4()
                    .flex()
                    .flex_col()
                    .gap_3p5()
                    // 1. Stream Actions Card
                    .child(
                        div()
                            .p_3p5()
                            .rounded(px(12.0))
                            .bg(rgb(0x0e0e0e))
                            .border_1()
                            .border_color(rgb(0x262626))
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgb(0xffffff))
                                            .child("Floating Window Mirrors"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(0x737373))
                                            .child(format!("{} active stream(s)", active_mirrors)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .when(active_mirrors > 0, |el| {
                                        el.child(
                                            div()
                                                .id("btn-toggle-pause-mirrors")
                                                .px_3()
                                                .py_1p5()
                                                .rounded(px(8.0))
                                                .bg(rgb(0x1a1a1a))
                                                .border_1()
                                                .border_color(rgb(0x333333))
                                                .text_xs()
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(rgb(0xffffff))
                                                .cursor_pointer()
                                                .hover(|s| {
                                                    s.bg(rgb(0x282828))
                                                        .border_color(rgb(0x444444))
                                                })
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.app_state.update(cx, |state, cx| {
                                                        state.toggle_pause_all(cx);
                                                    });
                                                }))
                                                .child(if all_paused { "Resume All" } else { "Pause All" }),
                                        )
                                        .child(
                                            div()
                                                .id("btn-close-all-mirrors")
                                                .px_3()
                                                .py_1p5()
                                                .rounded(px(8.0))
                                                .bg(rgb(0x1a1a1a))
                                                .border_1()
                                                .border_color(rgb(0x333333))
                                                .text_xs()
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(rgb(0xd4d4d4))
                                                .cursor_pointer()
                                                .hover(|s| {
                                                    s.bg(rgb(0xdc2626))
                                                        .border_color(rgb(0xef4444))
                                                        .text_color(rgb(0xffffff))
                                                })
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.app_state.update(cx, |state, cx| {
                                                        state.close_all_mirrors(cx);
                                                    });
                                                }))
                                                .child("Close All"),
                                        )
                                    })
                                    .child(
                                        div()
                                            .id("btn-spawn-mirror")
                                            .px_3p5()
                                            .py_1p5()
                                            .rounded(px(8.0))
                                            .bg(if is_picking {
                                                rgb(0xa3a3a3)
                                            } else {
                                                rgb(0xffffff)
                                            })
                                            .border_1()
                                            .border_color(rgb(0xffffff))
                                            .text_xs()
                                            .font_weight(FontWeight::EXTRA_BOLD)
                                            .text_color(rgb(0x000000))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(0xd4d4d4)))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                let state = this.app_state.clone();
                                                AppState::spawn_mirror(state, cx);
                                            }))
                                            .child(if is_picking {
                                                "Selecting Window…"
                                            } else {
                                                "+ New Mirror"
                                            }),
                                    ),
                            ),
                    )
                    // 2. Appearance & Hover Section
                    .child(
                        div()
                            .p_3p5()
                            .rounded(px(12.0))
                            .bg(rgb(0x0e0e0e))
                            .border_1()
                            .border_color(rgb(0x262626))
                            .flex()
                            .flex_col()
                            .gap_3()
                            // Hover Transparency Continuous Slider
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1p5()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(rgb(0xffffff))
                                                    .child("Hover Opacity"),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(FontWeight::EXTRA_BOLD)
                                                    .text_color(rgb(0xffffff))
                                                    .child(format!("{}%", opacity_pct)),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .id("hover-opacity-slider")
                                            .w(px(track_width))
                                            .h(px(22.0))
                                            .flex()
                                            .items_center()
                                            .relative()
                                            .cursor_pointer()
                                            .on_mouse_down(
                                                gpui::MouseButton::Left,
                                                cx.listener(
                                                    move |this, event: &MouseDownEvent, _, cx| {
                                                        cx.stop_propagation();
                                                        this.is_dragging_slider = true;
                                                        this.update_slider_from_x(
                                                            f32::from(event.position.x),
                                                            win_w,
                                                            cx,
                                                        );
                                                    },
                                                ),
                                            )
                                            .child(
                                                div()
                                                    .w(px(track_width))
                                                    .h(px(8.0))
                                                    .rounded(px(4.0))
                                                    .bg(rgb(0x202020))
                                                    .border_1()
                                                    .border_color(rgb(0x333333))
                                                    .relative()
                                                    .overflow_hidden()
                                                    .child(
                                                        div()
                                                            .w(px(fill_width))
                                                            .h_full()
                                                            .rounded(px(4.0))
                                                            .bg(rgb(0xffffff)),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .absolute()
                                                    .left(px((fill_width - 8.0).clamp(0.0, track_width - 16.0)))
                                                    .size(px(16.0))
                                                    .rounded_full()
                                                    .bg(rgb(0xffffff))
                                                    .border_2()
                                                    .border_color(rgb(0x000000))
                                                    .shadow_sm(),
                                            ),
                                    ),
                            )
                            // Animation Speed Selector
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(0xffffff))
                                            .child("Animation Speed"),
                                    )
                                    .child(
                                        div().flex().items_center().gap_1().children(
                                            AnimationSpeed::ALL.into_iter().map(|speed| {
                                                let selected = config.animation_speed == speed;
                                                div()
                                                    .id(speed.label())
                                                    .px_2p5()
                                                    .py_1()
                                                    .rounded(px(7.0))
                                                    .border_1()
                                                    .border_color(if selected {
                                                        rgb(0xffffff)
                                                    } else {
                                                        rgb(0x2c2c2c)
                                                    })
                                                    .text_xs()
                                                    .font_weight(if selected {
                                                        FontWeight::EXTRA_BOLD
                                                    } else {
                                                        FontWeight::MEDIUM
                                                    })
                                                    .bg(if selected {
                                                        rgb(0xffffff)
                                                    } else {
                                                        rgb(0x181818)
                                                    })
                                                    .text_color(if selected {
                                                        rgb(0x000000)
                                                    } else {
                                                        rgb(0xa3a3a3)
                                                    })
                                                    .cursor_pointer()
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.app_state.update(cx, |state, cx| {
                                                            state.update_config(cx, |cfg| {
                                                                 cfg.animation_speed = speed;
                                                            });
                                                        });
                                                    }))
                                                    .child(speed.label())
                                            }),
                                        ),
                                    ),
                            ),
                    )
                    // 3. Window Behavior & Performance Section
                    .child(
                        div()
                            .p_3p5()
                            .rounded(px(12.0))
                            .bg(rgb(0x0e0e0e))
                            .border_1()
                            .border_color(rgb(0x262626))
                            .flex()
                            .flex_col()
                            .gap_3()
                            // Always on Top toggle
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(rgb(0xffffff))
                                                    .child("Always on Top"),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(0x737373))
                                                    .child("Keep floating mirrors above other windows"),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .id("toggle-always-on-top")
                                            .px_3()
                                            .py_1()
                                            .rounded(px(12.0))
                                            .border_1()
                                            .border_color(if config.always_on_top {
                                                rgb(0xffffff)
                                            } else {
                                                rgb(0x2c2c2c)
                                            })
                                            .bg(if config.always_on_top {
                                                rgb(0xffffff)
                                            } else {
                                                rgb(0x181818)
                                            })
                                            .text_xs()
                                            .font_weight(FontWeight::EXTRA_BOLD)
                                            .text_color(if config.always_on_top {
                                                rgb(0x000000)
                                            } else {
                                                rgb(0x737373)
                                            })
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.app_state.update(cx, |state, cx| {
                                                    state.update_config(cx, |cfg| {
                                                        cfg.always_on_top = !cfg.always_on_top;
                                                    });
                                                });
                                            }))
                                            .child(if config.always_on_top { "ON" } else { "OFF" }),
                                    ),
                            )
                            // Default Spawn Size
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(0xffffff))
                                            .child("Default Mirror Size"),
                                    )
                                    .child(
                                        div().flex().items_center().gap_1().children(
                                            SpawnSize::ALL.into_iter().map(|sz| {
                                                let selected = config.default_spawn_size == sz;
                                                div()
                                                    .id(sz.label())
                                                    .px_2p5()
                                                    .py_1()
                                                    .rounded(px(7.0))
                                                    .border_1()
                                                    .border_color(if selected {
                                                        rgb(0xffffff)
                                                    } else {
                                                        rgb(0x2c2c2c)
                                                    })
                                                    .text_xs()
                                                    .font_weight(if selected {
                                                        FontWeight::EXTRA_BOLD
                                                    } else {
                                                        FontWeight::MEDIUM
                                                    })
                                                    .bg(if selected {
                                                        rgb(0xffffff)
                                                    } else {
                                                        rgb(0x181818)
                                                    })
                                                    .text_color(if selected {
                                                        rgb(0x000000)
                                                    } else {
                                                        rgb(0xa3a3a3)
                                                    })
                                                    .cursor_pointer()
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.app_state.update(cx, |state, cx| {
                                                            state.update_config(cx, |cfg| {
                                                                cfg.default_spawn_size = sz;
                                                            });
                                                        });
                                                    }))
                                                    .child(sz.label())
                                            }),
                                        ),
                                    ),
                            )
                            // Target FPS
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(0xffffff))
                                            .child("Target Frame Rate"),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .children([30u32, 60u32].into_iter().map(|fps_opt| {
                                                let selected = config.target_fps == fps_opt;
                                                div()
                                                    .id(if fps_opt == 30 {
                                                        "fps-30"
                                                    } else {
                                                        "fps-60"
                                                    })
                                                    .px_2p5()
                                                    .py_1()
                                                    .rounded(px(7.0))
                                                    .border_1()
                                                    .border_color(if selected {
                                                        rgb(0xffffff)
                                                    } else {
                                                        rgb(0x2c2c2c)
                                                    })
                                                    .text_xs()
                                                    .font_weight(if selected {
                                                        FontWeight::EXTRA_BOLD
                                                    } else {
                                                        FontWeight::MEDIUM
                                                    })
                                                    .bg(if selected {
                                                        rgb(0xffffff)
                                                    } else {
                                                        rgb(0x181818)
                                                    })
                                                    .text_color(if selected {
                                                        rgb(0x000000)
                                                    } else {
                                                        rgb(0xa3a3a3)
                                                    })
                                                    .cursor_pointer()
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.app_state.update(cx, |state, cx| {
                                                            state.update_config(cx, |cfg| {
                                                                cfg.target_fps = fps_opt;
                                                            });
                                                        });
                                                    }))
                                                    .child(format!("{} FPS", fps_opt))
                                            })),
                                    ),
                            ),
                    )
                    // 4. Separate Debug Options Section
                    .child(
                        div()
                            .p_3p5()
                            .rounded(px(12.0))
                            .bg(rgb(0x0a0a0a))
                            .border_1()
                            .border_color(rgb(0x262626))
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(0x737373))
                                    .child("Debug Options"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(rgb(0xffffff))
                                                    .child("Show FPS & Resolution Badge"),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(0x737373))
                                                    .child("Display live stream resolution and FPS overlay"),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .id("toggle-debug-fps-badge")
                                            .px_3()
                                            .py_1()
                                            .rounded(px(12.0))
                                            .border_1()
                                            .border_color(if config.show_fps_badge {
                                                rgb(0xffffff)
                                            } else {
                                                rgb(0x2c2c2c)
                                            })
                                            .bg(if config.show_fps_badge {
                                                rgb(0xffffff)
                                            } else {
                                                rgb(0x181818)
                                            })
                                            .text_xs()
                                            .font_weight(FontWeight::EXTRA_BOLD)
                                            .text_color(if config.show_fps_badge {
                                                rgb(0x000000)
                                            } else {
                                                rgb(0x737373)
                                            })
                                            .cursor_pointer()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.app_state.update(cx, |state, cx| {
                                                    state.update_config(cx, |cfg| {
                                                        cfg.show_fps_badge = !cfg.show_fps_badge;
                                                    });
                                                });
                                            }))
                                            .child(if config.show_fps_badge { "ON" } else { "OFF" }),
                                    ),
                            ),
                    ),
            )
    }
}
