use gpui::{
    div, prelude::*, px, rgb, rgba, Context, Entity, FontWeight, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Render, Window,
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

        Self {
            app_state,
            is_dragging_slider: false,
        }
    }

    fn update_slider_from_x(&mut self, mouse_x: f32, window_width: f32, cx: &mut Context<Self>) {
        let track_left = 40.0f32;
        let track_right = (window_width - 40.0).max(track_left + 100.0);
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
            Some((460.0, 560.0)),
            Some((460.0, 560.0)),
        );

        let win_w = f32::from(window.bounds().size.width).max(460.0);
        let (config, active_mirrors, is_picking) = {
            let state = self.app_state.read(cx);
            let count = state
                .mirrors
                .iter()
                .filter(|m| m.entity.upgrade().is_some())
                .count();
            (state.config.clone(), count, state.is_picking_window)
        };

        let opacity_pct = (config.hover_opacity * 100.0).round() as u32;
        let slider_ratio = ((config.hover_opacity - 0.05) / 0.95).clamp(0.0, 1.0);
        let track_width = (win_w - 80.0).max(200.0);
        let fill_width = (track_width * slider_ratio).round();

        div()
            .id("settings-root")
            .size_full()
            .bg(rgba(0x0d0f14f4))
            .rounded(px(16.0))
            .border_2()
            .border_color(rgba(0xffffff2e))
            .flex()
            .flex_col()
            .overflow_hidden()
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                if this.is_dragging_slider {
                    this.update_slider_from_x(f32::from(event.position.x), win_w, cx);
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
            // Top Draggable Dark Frosted Header
            .child(
                div()
                    .w_full()
                    .px_5()
                    .py_3p5()
                    .bg(rgba(0x161922e6))
                    .border_b_1()
                    .border_color(rgba(0xffffff18))
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
                                    .size(px(10.0))
                                    .rounded(px(5.0))
                                    .bg(rgb(0x10b981)),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::EXTRA_BOLD)
                                    .text_color(rgb(0xf9fafb))
                                    .child("aire"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0x9ca3af))
                                    .child("Control Panel"),
                            ),
                    )
                    // Frosted Glass Close Button with Bold Centered Icon
                    .child(
                        div()
                            .id("btn-close-settings")
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(30.0))
                            .rounded(px(15.0))
                            .bg(rgba(0x252a3abf))
                            .border_1()
                            .border_color(rgba(0xffffff45))
                            .shadow_sm()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgba(0xe11d48d9)).border_color(rgba(0xffb4c4aa)))
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
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(14.0))
                                    .line_height(px(14.0))
                                    .font_weight(FontWeight::EXTRA_BOLD)
                                    .text_color(rgb(0xffffff))
                                    .child("✕"),
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
                            .bg(rgba(0x181c26e6))
                            .border_1()
                            .border_color(rgba(0xffffff1a))
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
                                            .text_color(rgb(0xf9fafb))
                                            .child("Floating Window Mirrors"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(rgb(0x9ca3af))
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
                                                .id("btn-close-all-mirrors")
                                                .px_3()
                                                .py_1p5()
                                                .rounded(px(8.0))
                                                .bg(rgba(0x7f1d1d99))
                                                .border_1()
                                                .border_color(rgba(0xf8717155))
                                                .text_xs()
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(rgb(0xfca5a5))
                                                .cursor_pointer()
                                                .hover(|s| {
                                                    s.bg(rgba(0x991b1bcc))
                                                        .border_color(rgba(0xf87171aa))
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
                                                rgb(0x34d399)
                                            } else {
                                                rgb(0x10b981)
                                            })
                                            .border_1()
                                            .border_color(rgba(0xffffff40))
                                            .text_xs()
                                            .font_weight(FontWeight::EXTRA_BOLD)
                                            .text_color(rgb(0x052e16))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(0x34d399)))
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
                            .bg(rgba(0x181c26e6))
                            .border_1()
                            .border_color(rgba(0xffffff1a))
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
                                                    .text_color(rgb(0xf3f4f6))
                                                    .child("Hover Opacity"),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .font_weight(FontWeight::EXTRA_BOLD)
                                                    .text_color(rgb(0x34d399))
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
                                                    .bg(rgb(0x262b3c))
                                                    .border_1()
                                                    .border_color(rgba(0xffffff18))
                                                    .relative()
                                                    .overflow_hidden()
                                                    .child(
                                                        div()
                                                            .w(px(fill_width))
                                                            .h_full()
                                                            .rounded(px(4.0))
                                                            .bg(rgb(0x10b981)),
                                                    ),
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
                                            .text_color(rgb(0xf3f4f6))
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
                                                        rgba(0x34d39988)
                                                    } else {
                                                        rgba(0xffffff18)
                                                    })
                                                    .text_xs()
                                                    .font_weight(if selected {
                                                        FontWeight::EXTRA_BOLD
                                                    } else {
                                                        FontWeight::MEDIUM
                                                    })
                                                    .bg(if selected {
                                                        rgb(0x10b981)
                                                    } else {
                                                        rgb(0x232838)
                                                    })
                                                    .text_color(if selected {
                                                        rgb(0x052e16)
                                                    } else {
                                                        rgb(0xd1d5db)
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
                            .bg(rgba(0x181c26e6))
                            .border_1()
                            .border_color(rgba(0xffffff1a))
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
                                                    .text_color(rgb(0xf3f4f6))
                                                    .child("Always on Top"),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(0x9ca3af))
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
                                                rgba(0x34d39988)
                                            } else {
                                                rgba(0xffffff22)
                                            })
                                            .bg(if config.always_on_top {
                                                rgb(0x10b981)
                                            } else {
                                                rgb(0x262b3c)
                                            })
                                            .text_xs()
                                            .font_weight(FontWeight::EXTRA_BOLD)
                                            .text_color(if config.always_on_top {
                                                rgb(0x052e16)
                                            } else {
                                                rgb(0x9ca3af)
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
                                            .text_color(rgb(0xf3f4f6))
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
                                                        rgba(0x34d39988)
                                                    } else {
                                                        rgba(0xffffff18)
                                                    })
                                                    .text_xs()
                                                    .font_weight(if selected {
                                                        FontWeight::EXTRA_BOLD
                                                    } else {
                                                        FontWeight::MEDIUM
                                                    })
                                                    .bg(if selected {
                                                        rgb(0x10b981)
                                                    } else {
                                                        rgb(0x232838)
                                                    })
                                                    .text_color(if selected {
                                                        rgb(0x052e16)
                                                    } else {
                                                        rgb(0xd1d5db)
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
                                            .text_color(rgb(0xf3f4f6))
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
                                                        rgba(0x34d39988)
                                                    } else {
                                                        rgba(0xffffff18)
                                                    })
                                                    .text_xs()
                                                    .font_weight(if selected {
                                                        FontWeight::EXTRA_BOLD
                                                    } else {
                                                        FontWeight::MEDIUM
                                                    })
                                                    .bg(if selected {
                                                        rgb(0x10b981)
                                                    } else {
                                                        rgb(0x232838)
                                                    })
                                                    .text_color(if selected {
                                                        rgb(0x052e16)
                                                    } else {
                                                        rgb(0xd1d5db)
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
                            .bg(rgba(0x13161fe6))
                            .border_1()
                            .border_color(rgba(0xffffff16))
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(0x9ca3af))
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
                                                    .text_color(rgb(0xf3f4f6))
                                                    .child("Show FPS & Resolution Badge"),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(0x9ca3af))
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
                                                rgba(0x34d39988)
                                            } else {
                                                rgba(0xffffff22)
                                            })
                                            .bg(if config.show_fps_badge {
                                                rgb(0x10b981)
                                            } else {
                                                rgb(0x262b3c)
                                            })
                                            .text_xs()
                                            .font_weight(FontWeight::EXTRA_BOLD)
                                            .text_color(if config.show_fps_badge {
                                                rgb(0x052e16)
                                            } else {
                                                rgb(0x9ca3af)
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
