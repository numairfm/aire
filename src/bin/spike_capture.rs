use std::{
    os::fd::OwnedFd,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context as AnyhowContext, Result};
use ashpd::{
    desktop::screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType, StartCastOptions},
    enumflags2::BitFlags,
};
use crossbeam_channel::{bounded, Receiver, Sender};
use gpui::{
    div, prelude::*, px, rgb, rgba, size, App, Bounds, Context, FontWeight,
    Point, Render, RenderImage, ResizeEdge, Size, WeakEntity, Window, WindowBackgroundAppearance,
    WindowBounds, WindowDecorations, WindowKind, WindowOptions,
};
use gpui_platform::application;
use image::{Frame, ImageBuffer};
use smallvec::smallvec;
use pipewire as pw;
use pw::{properties::properties, spa};
use spa::pod::Pod;

struct CapturedFrame {
    width: u32,
    height: u32,
    render_image: Arc<RenderImage>,
}

struct StreamUserData {
    format: spa::param::video::VideoInfoRaw,
    frame_tx: Sender<CapturedFrame>,
    is_paused: Arc<AtomicBool>,
    fps_counter: u32,
    last_fps_log: Instant,
}

struct MirrorView {
    current_image: Option<Arc<RenderImage>>,
    source_width: u32,
    source_height: u32,
    aspect_ratio: f32,
    fps: f64,
    is_minimized: bool,
    is_paused: Arc<AtomicBool>,
}

impl MirrorView {
    fn new(
        cx: &mut Context<Self>,
        frame_rx: Receiver<CapturedFrame>,
        is_paused: Arc<AtomicBool>,
    ) -> Self {
        // Task to receive frames from PipeWire thread
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let mut frames_this_sec = 0;
            let mut last_fps_time = Instant::now();

            loop {
                // Read next frame from channel (non-blocking / short timeout)
                match frame_rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(frame) => {
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
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                        // idle / static content: yield briefly
                        cx.background_executor().timer(Duration::from_millis(16)).await;
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                        eprintln!("[MIRROR] Capture channel closed");
                        break;
                    }
                }
            }
        })
        .detach();

        Self {
            current_image: None,
            source_width: 1920,
            source_height: 1080,
            aspect_ratio: 16.0 / 9.0,
            fps: 0.0,
            is_minimized: false,
            is_paused,
        }
    }
}

impl Render for MirrorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.is_minimized {
            // Compact 36px pill restore UI
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
                    this.is_paused.store(false, Ordering::Relaxed);
                    let target_h = 400.0 / this.aspect_ratio;
                    window.resize(size(px(400.0), px(target_h)));
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

        // Full mirror HUD
        div()
            .group("hud")
            .size_full()
            .relative()
            // Light Frosty Glass styling
            .bg(rgba(0xffffff24))
            .rounded(px(10.0))
            .border_1()
            .border_color(rgba(0xffffff48))
            .shadow_xl()
            .overflow_hidden()
            // Click-drag anywhere to move
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
                            .text_color(rgb(0x444444))
                            .child("Waiting for source window...")
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
                                this.is_paused.store(true, Ordering::Relaxed);
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
            // HUD Status (Bottom-left)
            .child(
                div()
                    .absolute()
                    .bottom_2()
                    .left_2()
                    .px_2()
                    .py_1()
                    .rounded(px(6.0))
                    .bg(rgba(0x00000099))
                    .text_xs()
                    .text_color(rgb(0xffffff))
                    .opacity(0.6)
                    .group_hover("hud", |s| s.opacity(0.95))
                    .child(format!(
                        "{}×{} | {:.0} fps",
                        self.source_width, self.source_height, self.fps
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

fn start_pipewire_capture(
    node_id: u32,
    stream_fd: Option<OwnedFd>,
    frame_tx: Sender<CapturedFrame>,
    is_paused: Arc<AtomicBool>,
) -> Result<()> {
    pw::init();

    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;

    let core = if let Some(fd) = stream_fd {
        context.connect_fd_rc(fd, None)?
    } else {
        context.connect_rc(None)?
    };

    let user_data = StreamUserData {
        format: Default::default(),
        frame_tx,
        is_paused,
        fps_counter: 0,
        last_fps_log: Instant::now(),
    };

    let stream = pw::stream::StreamBox::new(
        &core,
        "aire-capture",
        properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )?;

    let _listener = stream
        .add_local_listener_with_user_data(user_data)
        .state_changed(|_, _, old, new| {
            eprintln!("[PIPEWIRE] Stream state: {:?} -> {:?}", old, new);
        })
        .param_changed(|_, user_data, id, param| {
            let Some(param) = param else { return };
            if id != pw::spa::param::ParamType::Format.as_raw() {
                return;
            }

            let (media_type, media_subtype) = match pw::spa::param::format_utils::parse_format(param) {
                Ok(v) => v,
                Err(_) => return,
            };

            if media_type != pw::spa::param::format::MediaType::Video
                || media_subtype != pw::spa::param::format::MediaSubtype::Raw
            {
                return;
            }

            if user_data.format.parse(param).is_ok() {
                let size = user_data.format.size();
                eprintln!(
                    "[PIPEWIRE] Negotiated format: {:?}, size: {}x{}, framerate: {}/{}",
                    user_data.format.format(),
                    size.width,
                    size.height,
                    user_data.format.framerate().num,
                    user_data.format.framerate().denom
                );
            }
        })
        .process(|stream, user_data| {
            if user_data.is_paused.load(Ordering::Relaxed) {
                if let Some(buf) = stream.dequeue_buffer() {
                    drop(buf);
                }
                return;
            }

            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };

            let datas = buffer.datas_mut();
            if datas.is_empty() {
                return;
            }

            let width = user_data.format.size().width;
            let height = user_data.format.size().height;
            if width == 0 || height == 0 {
                return;
            }

            let data = &mut datas[0];
            let chunk_size = data.chunk().size() as usize;
            let Some(slice) = data.data() else {
                return;
            };

            if slice.len() < chunk_size || chunk_size < (width * height * 4) as usize {
                return;
            }

            // Convert pixel format to RGBA for GPUI
            // ScreenCast portal video format is typically BGRx / BGRA or RGBA
            let raw_bytes = &slice[..chunk_size];
            let is_bgr = match user_data.format.format() {
                spa::param::video::VideoFormat::BGRx | spa::param::video::VideoFormat::BGRA => true,
                _ => false,
            };

            let mut rgba_buf = vec![0u8; (width * height * 4) as usize];
            if is_bgr {
                for i in (0..(width * height * 4) as usize).step_by(4) {
                    rgba_buf[i] = raw_bytes[i + 2];     // R
                    rgba_buf[i + 1] = raw_bytes[i + 1]; // G
                    rgba_buf[i + 2] = raw_bytes[i];     // B
                    rgba_buf[i + 3] = 255;              // A
                }
            } else {
                rgba_buf.copy_from_slice(&raw_bytes[..(width * height * 4) as usize]);
            }

            if let Some(img_buffer) = ImageBuffer::from_raw(width, height, rgba_buf) {
                let render_image = Arc::new(RenderImage::new(smallvec![Frame::new(img_buffer)]));
                let _ = user_data.frame_tx.try_send(CapturedFrame {
                    width,
                    height,
                    render_image,
                });
            }

            user_data.fps_counter += 1;
            let now = Instant::now();
            if now.duration_since(user_data.last_fps_log) >= Duration::from_secs(2) {
                let fps = user_data.fps_counter as f64 / now.duration_since(user_data.last_fps_log).as_secs_f64();
                eprintln!("[PIPEWIRE] Live stream receiving at {:.1} FPS ({}x{})", fps, width, height);
                user_data.fps_counter = 0;
                user_data.last_fps_log = now;
            }
        })
        .register()?;

    // Format filter: Negotiate raw video, BGRx/BGRA/RGBA
    let obj = pw::spa::pod::object!(
        pw::spa::utils::SpaTypes::ObjectParamFormat,
        pw::spa::param::ParamType::EnumFormat,
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::MediaType,
            Id,
            pw::spa::param::format::MediaType::Video
        ),
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::MediaSubtype,
            Id,
            pw::spa::param::format::MediaSubtype::Raw
        ),
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            pw::spa::param::video::VideoFormat::BGRx,
            pw::spa::param::video::VideoFormat::BGRx,
            pw::spa::param::video::VideoFormat::BGRA,
            pw::spa::param::video::VideoFormat::RGBA,
            pw::spa::param::video::VideoFormat::RGBx,
        ),
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            pw::spa::utils::Rectangle { width: 1920, height: 1080 },
            pw::spa::utils::Rectangle { width: 1, height: 1 },
            pw::spa::utils::Rectangle { width: 4096, height: 4096 }
        ),
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            pw::spa::utils::Fraction { num: 60, denom: 1 },
            pw::spa::utils::Fraction { num: 0, denom: 1 },
            pw::spa::utils::Fraction { num: 60, denom: 1 }
        ),
    );

    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(obj),
    )?
    .0
    .into_inner();

    let mut params = [Pod::from_bytes(&values).context("Failed to parse Pod params")?];

    stream.connect(
        spa::utils::Direction::Input,
        Some(node_id),
        pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
        &mut params,
    )?;

    eprintln!("[PIPEWIRE] Loop starting, connected to node {}", node_id);
    mainloop.run();
    Ok(())
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

fn main() -> Result<()> {
    configure_kwin_rules_if_needed();

    // Channel for transferring captured frames from PipeWire thread to GPUI
    let (frame_tx, frame_rx) = bounded::<CapturedFrame>(2);
    let is_paused = Arc::new(AtomicBool::new(false));
    let is_paused_for_thread = is_paused.clone();

    // Request portal screencast synchronously or in background thread before UI loop
    // This pops up the KDE native window picker dialog
    let (node_id, pw_fd) = async_io::block_on(async {
        eprintln!("[PORTAL] Requesting ScreenCast portal session...");
        let proxy = Screencast::new().await.context("Failed to connect to ScreenCast portal")?;
        let session = proxy.create_session(Default::default()).await.context("Failed to create screencast session")?;

        let options = SelectSourcesOptions::default()
            .set_cursor_mode(CursorMode::Metadata)
            .set_sources(BitFlags::from_flag(SourceType::Window))
            .set_multiple(false);

        proxy.select_sources(&session, options).await.context("SelectSources request failed")?;

        eprintln!("[PORTAL] Presenting native KDE window picker dialog...");
        let request = proxy.start(&session, None, StartCastOptions::default()).await?;
        let streams = request.response()?.streams().to_vec();
        let stream = streams.first().context("No window selected from portal picker")?;
        let node_id = stream.pipe_wire_node_id();

        eprintln!("[PORTAL] Window picked! PipeWire node ID: {}", node_id);
        let fd_res = proxy.open_pipe_wire_remote(&session, Default::default()).await;
        let fd = fd_res.ok();

        Ok::<(u32, Option<OwnedFd>), anyhow::Error>((node_id, fd))
    })?;

    // Spawn PipeWire capture thread
    thread::spawn(move || {
        if let Err(e) = start_pipewire_capture(node_id, pw_fd, frame_tx, is_paused_for_thread) {
            eprintln!("[PIPEWIRE] Capture thread error: {:?}", e);
        }
    });

    // Run GPUI App
    application().run(|cx: &mut App| {
        let display_bounds = cx.displays().first().map(|d| d.bounds()).unwrap_or_else(|| {
            Bounds {
                origin: Point::new(px(0.0), px(0.0)),
                size: Size::new(px(1920.0), px(1080.0)),
            }
        });

        let win_width = px(400.0);
        let win_height = px(225.0);

        // Centered on screen by default as requested
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

        cx.open_window(options, move |_, cx| {
            cx.activate(false);
            cx.new(|cx| MirrorView::new(cx, frame_rx, is_paused))
        })
        .expect("Failed to open GPUI mirror window");
    });

    Ok(())
}
