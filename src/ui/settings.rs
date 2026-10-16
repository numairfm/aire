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
        let track_left = 24.0f32;
        let track_right = (window_width - 24.0).max(track_left + 100.0);
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
            Some((460.0, 520.0)),
            Some((460.0, 520.0)),
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
        let track_width = (win_w - 48.0).max(200.0);
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
            // Top Integrated Handle Bar (Seamless with window surface, no border/color split)
            .child(
                div()
                    .w_full()
                    .px_6()
                    .pt_5()
                    .pb_2()
                    .flex()
                    .items_center()
                    .justify_between()
                    .cursor_grab()
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                        window.start_window_move();
                    })
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(0xffffff))
                            .child("aire"),
                    )
                    .child(
                        div()
                            .id("btn-close-settings")
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(28.0))
                            .rounded_full()
                            .bg(rgb(0x141414))
                            .border_1()
                            .border_color(rgb(0x2a2a2a))
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
                                        let r = px(4.0);
                                        let mut b1 = PathBuilder::stroke(px(2.0));
                                        b1.move_to(point(center_x - r, center_y - r));
                                        b1.line_to(point(center_x + r, center_y + r));
                                        if let Ok(path) = b1.build() {
                                            window.paint_path(path, rgb(0xffffff));
                                        }
                                        let mut b2 = PathBuilder::stroke(px(2.0));
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
            // Body Content - Minimal flat black layout organized with clean dividers
            .child(
                div()
                    .flex_1()
                    .px_6()
                    .pt_2()
                    .pb_5()
                    .flex()
                    .flex_col()
                    .justify_between()
                    // 1. Mirrors Status & Actions
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgb(0xffffff))
                                            .child("Floating Mirrors"),
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
                                                .px_3p5()
                                                .py_1p5()
                                                .rounded_full()
                                                .bg(rgb(0x141414))
                                                .border_1()
                                                .border_color(rgb(0x2a2a2a))
                                                .text_xs()
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(rgb(0xd4d4d4))
                                                .cursor_pointer()
                                                .hover(|s| {
                                                    s.bg(rgb(0x222222))
                                                        .border_color(rgb(0x3a3a3a))
                                                        .text_color(rgb(0xffffff))
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
                                                .px_3p5()
                                                .py_1p5()
                                                .rounded_full()
                                                .bg(rgb(0x141414))
                                                .border_1()
                                                .border_color(rgb(0x2a2a2a))
                                                .text_xs()
                                                .font_weight(FontWeight::MEDIUM)
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
                                            .px_4()
                                            .py_1p5()
                                            .rounded_full()
                                            .bg(if is_picking {
                                                rgb(0x888888)
                                            } else {
                                                rgb(0xffffff)
                                            })
                                            .border_1()
                                            .border_color(rgb(0xffffff))
                                            .text_xs()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgb(0x000000))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(0xd4d4d4)))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                let state = this.app_state.clone();
                                                AppState::spawn_mirror(state, cx);
                                            }))
                                            .child(if is_picking {
                                                "Selecting…"
                                            } else {
                                                "+ New Mirror"
                                            }),
                                    ),
                            ),
                    )
                    // Divider
                    .child(div().w_full().h(px(1.0)).bg(rgb(0x1a1a1a)))
                    // 2. Appearance & Hover
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_3()
                            // Hover Opacity Continuous Slider
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(rgb(0xd4d4d4))
                                                    .child("Hover Opacity"),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(rgb(0xffffff))
                                                    .child(format!("{}%", opacity_pct)),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .id("hover-opacity-slider")
                                            .w(px(track_width))
                                            .h(px(20.0))
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
                                                    .h(px(6.0))
                                                    .rounded_full()
                                                    .bg(rgb(0x202020))
                                                    .relative()
                                                    .overflow_hidden()
                                                    .child(
                                                        div()
                                                            .w(px(fill_width))
                                                            .h_full()
                                                            .rounded_full()
                                                            .bg(rgb(0xffffff)),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .absolute()
                                                    .left(px((fill_width - 7.0).clamp(0.0, track_width - 14.0)))
                                                    .size(px(14.0))
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
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(0xd4d4d4))
                                            .child("Animation Speed"),
                                    )
                                    .child(
                                        div().flex().items_center().gap_1p5().children(
                                            AnimationSpeed::ALL.into_iter().map(|speed| {
                                                let selected = config.animation_speed == speed;
                                                div()
                                                    .id(speed.label())
                                                    .px_3()
                                                    .py_1()
                                                    .rounded_full()
                                                    .border_1()
                                                    .border_color(if selected {
                                                        rgb(0xffffff)
                                                    } else {
                                                        rgb(0x262626)
                                                    })
                                                    .text_xs()
                                                    .font_weight(if selected {
                                                        FontWeight::BOLD
                                                    } else {
                                                        FontWeight::MEDIUM
                                                    })
                                                    .bg(if selected {
                                                        rgb(0xffffff)
                                                    } else {
                                                        rgb(0x121212)
                                                    })
                                                    .text_color(if selected {
                                                        rgb(0x000000)
                                                    } else {
                                                        rgb(0x888888)
                                                    })
                                                    .cursor_pointer()
                                                    .hover(|s| {
                                                        if !selected {
                                                            s.border_color(rgb(0x3a3a3a)).text_color(rgb(0xcccccc))
                                                        } else {
                                                            s
                                                        }
                                                    })
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
                    // Divider
                    .child(div().w_full().h(px(1.0)).bg(rgb(0x1a1a1a)))
                    // 3. Window Behavior
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_3()
                            // Always on Top
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_0p5()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(rgb(0xd4d4d4))
                                                    .child("Always on Top"),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(0x666666))
                                                    .child("Keep floating mirrors above other windows"),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .id("toggle-always-on-top")
                                            .px_3p5()
                                            .py_1()
                                            .rounded_full()
                                            .border_1()
                                            .border_color(if config.always_on_top {
                                                rgb(0xffffff)
                                            } else {
                                                rgb(0x262626)
                                            })
                                            .bg(if config.always_on_top {
                                                rgb(0xffffff)
                                            } else {
                                                rgb(0x121212)
                                            })
                                            .text_xs()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(if config.always_on_top {
                                                rgb(0x000000)
                                            } else {
                                                rgb(0x666666)
                                            })
                                            .cursor_pointer()
                                            .hover(|s| {
                                                if !config.always_on_top {
                                                    s.border_color(rgb(0x3a3a3a)).text_color(rgb(0xaaaaaa))
                                                } else {
                                                    s
                                                }
                                            })
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
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(0xd4d4d4))
                                            .child("Default Mirror Size"),
                                    )
                                    .child(
                                        div().flex().items_center().gap_1p5().children(
                                            SpawnSize::ALL.into_iter().map(|sz| {
                                                let selected = config.default_spawn_size == sz;
                                                div()
                                                    .id(sz.label())
                                                    .px_3()
                                                    .py_1()
                                                    .rounded_full()
                                                    .border_1()
                                                    .border_color(if selected {
                                                        rgb(0xffffff)
                                                    } else {
                                                        rgb(0x262626)
                                                    })
                                                    .text_xs()
                                                    .font_weight(if selected {
                                                        FontWeight::BOLD
                                                    } else {
                                                        FontWeight::MEDIUM
                                                    })
                                                    .bg(if selected {
                                                        rgb(0xffffff)
                                                    } else {
                                                        rgb(0x121212)
                                                    })
                                                    .text_color(if selected {
                                                        rgb(0x000000)
                                                    } else {
                                                        rgb(0x888888)
                                                    })
                                                    .cursor_pointer()
                                                    .hover(|s| {
                                                        if !selected {
                                                            s.border_color(rgb(0x3a3a3a)).text_color(rgb(0xcccccc))
                                                        } else {
                                                            s
                                                        }
                                                    })
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
                            ),
                    )
                    // Divider
                    .child(div().w_full().h(px(1.0)).bg(rgb(0x1a1a1a)))
                    // 4. Performance & Display
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_3()
                            // Target FPS
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(0xd4d4d4))
                                            .child("Target Frame Rate"),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1p5()
                                            .children([30u32, 60u32].into_iter().map(|fps_opt| {
                                                let selected = config.target_fps == fps_opt;
                                                div()
                                                    .id(if fps_opt == 30 {
                                                        "fps-30"
                                                    } else {
                                                        "fps-60"
                                                    })
                                                    .px_3()
                                                    .py_1()
                                                    .rounded_full()
                                                    .border_1()
                                                    .border_color(if selected {
                                                        rgb(0xffffff)
                                                    } else {
                                                        rgb(0x262626)
                                                    })
                                                    .text_xs()
                                                    .font_weight(if selected {
                                                        FontWeight::BOLD
                                                    } else {
                                                        FontWeight::MEDIUM
                                                    })
                                                    .bg(if selected {
                                                        rgb(0xffffff)
                                                    } else {
                                                        rgb(0x121212)
                                                    })
                                                    .text_color(if selected {
                                                        rgb(0x000000)
                                                    } else {
                                                        rgb(0x888888)
                                                    })
                                                    .cursor_pointer()
                                                    .hover(|s| {
                                                        if !selected {
                                                            s.border_color(rgb(0x3a3a3a)).text_color(rgb(0xcccccc))
                                                        } else {
                                                            s
                                                        }
                                                    })
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
                            )
                            // FPS & Resolution Badge Toggle
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_0p5()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(rgb(0xd4d4d4))
                                                    .child("Show FPS & Resolution Badge"),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(0x666666))
                                                    .child("Display live stream resolution and FPS overlay"),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .id("toggle-debug-fps-badge")
                                            .px_3p5()
                                            .py_1()
                                            .rounded_full()
                                            .border_1()
                                            .border_color(if config.show_fps_badge {
                                                rgb(0xffffff)
                                            } else {
                                                rgb(0x262626)
                                            })
                                            .bg(if config.show_fps_badge {
                                                rgb(0xffffff)
                                            } else {
                                                rgb(0x121212)
                                            })
                                            .text_xs()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(if config.show_fps_badge {
                                                rgb(0x000000)
                                            } else {
                                                rgb(0x666666)
                                            })
                                            .cursor_pointer()
                                            .hover(|s| {
                                                if !config.show_fps_badge {
                                                    s.border_color(rgb(0x3a3a3a)).text_color(rgb(0xaaaaaa))
                                                } else {
                                                    s
                                                }
                                            })
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
