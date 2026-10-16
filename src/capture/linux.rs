use std::{
    os::fd::OwnedFd,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use anyhow::{Context, Result};
use ashpd::{
    desktop::screencast::{
        CursorMode, Screencast, SelectSourcesOptions, SourceType, StartCastOptions,
    },
    enumflags2::BitFlags,
};
use image::{Frame, ImageBuffer};
use pipewire as pw;
use pw::{properties::properties, spa};
use smallvec::smallvec;
use spa::pod::Pod;

use super::{CaptureEvent, CaptureSender, CapturedFrame, WindowCapture};

pub struct LinuxWindowCapture {
    is_paused: Arc<AtomicBool>,
    is_running: Arc<AtomicBool>,
    target_fps: Arc<AtomicU32>,
}

impl LinuxWindowCapture {
    pub fn start(
        event_tx: CaptureSender,
        initial_target_fps: u32,
    ) -> Result<Option<(Box<dyn WindowCapture>, Option<(u32, u32)>)>> {
        eprintln!("[PORTAL] Requesting window selection from XDG Desktop Portal...");
        let portal_res = async_io::block_on(async {
            let proxy = Screencast::new()
                .await
                .context("Failed to connect to ScreenCast portal")?;
            let session = proxy
                .create_session(Default::default())
                .await
                .context("Failed to create screencast session")?;

            let options = SelectSourcesOptions::default()
                .set_cursor_mode(CursorMode::Metadata)
                .set_sources(BitFlags::from_flag(SourceType::Window))
                .set_multiple(false);

            proxy
                .select_sources(&session, options)
                .await
                .context("SelectSources failed")?;

            let request = proxy
                .start(&session, None, StartCastOptions::default())
                .await?;
            let response = match request.response() {
                Ok(resp) => resp,
                Err(err) => {
                    eprintln!("[PORTAL] Portal dialog was cancelled: {:?}", err);
                    return Ok(None);
                }
            };

            let streams = response.streams().to_vec();
            let stream = match streams.first() {
                Some(s) => s,
                None => {
                    eprintln!("[PORTAL] No window stream returned.");
                    return Ok(None);
                }
            };
            let node_id = stream.pipe_wire_node_id();
            let pw_fd = proxy
                .open_pipe_wire_remote(&session, Default::default())
                .await
                .ok();
            Ok::<Option<(u32, Option<OwnedFd>, _, _)>, anyhow::Error>(Some((
                node_id, pw_fd, proxy, session,
            )))
        });

        let (node_id, pw_fd, _proxy, session) = match portal_res {
            Ok(Some(data)) => data,
            Ok(None) => return Ok(None),
            Err(err) => {
                eprintln!("[PORTAL] Portal error: {:#}", err);
                return Ok(None);
            }
        };

        eprintln!("[PIPEWIRE] Selected window stream node ID: {}", node_id);
        let is_paused = Arc::new(AtomicBool::new(false));
        let is_running = Arc::new(AtomicBool::new(true));
        let target_fps = Arc::new(AtomicU32::new(initial_target_fps.clamp(15, 120)));

        let is_paused_clone = is_paused.clone();
        let is_running_clone = is_running.clone();
        let target_fps_clone = target_fps.clone();
        let tx = event_tx.clone();
        let (dim_tx, dim_rx) = crossbeam_channel::bounded::<(u32, u32)>(1);

        thread::spawn(move || {
            let _keep_proxy = _proxy;
            if let Err(err) = run_pipewire_loop(
                node_id,
                pw_fd,
                tx.clone(),
                is_paused_clone,
                is_running_clone,
                target_fps_clone,
                Some(dim_tx),
            ) {
                let _ = tx.try_send(CaptureEvent::Error(format!(
                    "PipeWire stream error: {}",
                    err
                )));
            }
            let _ = async_io::block_on(session.close());
        });

        // Wait briefly (up to 150ms) for format negotiation so window opens at exact aspect ratio immediately
        let initial_dims = dim_rx.recv_timeout(Duration::from_millis(150)).ok();
        if let Some((w, h)) = initial_dims {
            eprintln!(
                "[PIPEWIRE] Initial source window dimensions: {}x{} (ratio: {:.3})",
                w,
                h,
                w as f32 / h as f32
            );
        }

        Ok(Some((
            Box::new(LinuxWindowCapture {
                is_paused,
                is_running,
                target_fps,
            }),
            initial_dims,
        )))
    }
}

impl WindowCapture for LinuxWindowCapture {
    fn pause(&self) {
        self.is_paused.store(true, Ordering::Relaxed);
    }

    fn resume(&self) {
        self.is_paused.store(false, Ordering::Relaxed);
    }

    fn stop(&self) {
        self.is_running.store(false, Ordering::Relaxed);
    }

    fn set_target_fps(&self, fps: u32) {
        self.target_fps.store(fps.clamp(15, 120), Ordering::Relaxed);
    }
}

impl Drop for LinuxWindowCapture {
    fn drop(&mut self) {
        self.is_running.store(false, Ordering::Relaxed);
    }
}

struct StreamContext {
    mainloop: pw::main_loop::MainLoopWeak,
    format: spa::param::video::VideoInfoRaw,
    event_tx: CaptureSender,
    initial_dim_tx: Option<crossbeam_channel::Sender<(u32, u32)>>,
    is_paused: Arc<AtomicBool>,
    is_running: Arc<AtomicBool>,
    target_fps: Arc<AtomicU32>,
    last_width: u32,
    last_height: u32,
    last_frame_sent: std::time::Instant,
}

fn run_pipewire_loop(
    node_id: u32,
    stream_fd: Option<OwnedFd>,
    event_tx: CaptureSender,
    is_paused: Arc<AtomicBool>,
    is_running: Arc<AtomicBool>,
    target_fps: Arc<AtomicU32>,
    initial_dim_tx: Option<crossbeam_channel::Sender<(u32, u32)>>,
) -> Result<()> {
    pw::init();

    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;

    let core = if let Some(fd) = stream_fd {
        context.connect_fd_rc(fd, None)?
    } else {
        context.connect_rc(None)?
    };

    // Periodic timer so mainloop exits promptly even when source window is static
    let mainloop_weak = mainloop.downgrade();
    let is_running_timer = is_running.clone();
    let _quit_timer = mainloop.loop_().add_timer(move |_| {
        if !is_running_timer.load(Ordering::Relaxed) {
            if let Some(ml) = mainloop_weak.upgrade() {
                ml.quit();
            }
        }
    });
    let _ = _quit_timer.update_timer(
        Some(Duration::from_millis(200)),
        Some(Duration::from_millis(200)),
    );

    let stream_ctx = StreamContext {
        mainloop: mainloop.downgrade(),
        format: Default::default(),
        event_tx: event_tx.clone(),
        initial_dim_tx,
        is_paused,
        is_running: is_running.clone(),
        target_fps,
        last_width: 0,
        last_height: 0,
        last_frame_sent: std::time::Instant::now() - Duration::from_secs(1),
    };

    let stream = pw::stream::StreamBox::new(
        &core,
        "aire-mirror",
        properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )?;

    let _listener = stream
        .add_local_listener_with_user_data(stream_ctx)
        .state_changed(|_, ctx, old, new| {
            eprintln!("[PIPEWIRE] State changed: {:?} -> {:?}", old, new);
            if matches!(
                new,
                pw::stream::StreamState::Unconnected | pw::stream::StreamState::Error(_)
            ) {
                let _ = ctx.event_tx.try_send(CaptureEvent::SourceClosed);
                if let Some(ml) = ctx.mainloop.upgrade() {
                    ml.quit();
                }
            }
        })
        .param_changed(|_, ctx, id, param| {
            let Some(param) = param else { return };
            if id != pw::spa::param::ParamType::Format.as_raw() {
                return;
            }

            let (media_type, media_subtype) =
                match pw::spa::param::format_utils::parse_format(param) {
                    Ok(v) => v,
                    Err(_) => return,
                };

            if media_type != pw::spa::param::format::MediaType::Video
                || media_subtype != pw::spa::param::format::MediaSubtype::Raw
            {
                return;
            }

            if ctx.format.parse(param).is_ok() {
                let size = ctx.format.size();
                if let Some(dim_tx) = ctx.initial_dim_tx.take() {
                    let _ = dim_tx.try_send((size.width, size.height));
                }
                if size.width != ctx.last_width || size.height != ctx.last_height {
                    ctx.last_width = size.width;
                    ctx.last_height = size.height;
                    let _ = ctx.event_tx.try_send(CaptureEvent::Resized {
                        width: size.width,
                        height: size.height,
                    });
                }
            }
        })
        .process(|stream, ctx| {
            // Always dequeue the buffer first so PipeWire's buffer pool never stalls
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };

            if !ctx.is_running.load(Ordering::Relaxed) {
                if let Some(ml) = ctx.mainloop.upgrade() {
                    ml.quit();
                }
                return;
            }

            if ctx.is_paused.load(Ordering::Relaxed) {
                return;
            }

            // Do not allocate heap memory or create RenderImages if the UI hasn't consumed the queued frame yet
            if ctx.event_tx.is_full() {
                return;
            }

            let fps = ctx.target_fps.load(Ordering::Relaxed).max(15);
            let min_interval = Duration::from_micros((1_000_000 / (fps as u64)).saturating_sub(500));
            let now = std::time::Instant::now();
            if now.duration_since(ctx.last_frame_sent) < min_interval {
                return;
            }

            let datas = buffer.datas_mut();
            if datas.is_empty() {
                return;
            }

            let width = ctx.format.size().width;
            let height = ctx.format.size().height;
            if width == 0 || height == 0 {
                return;
            }

            let data = &mut datas[0];
            let chunk_size = data.chunk().size() as usize;
            let stride = data.chunk().stride() as usize;
            let Some(slice) = data.data() else {
                return;
            };

            let row_bytes = (width as usize) * 4;
            let src_stride = if stride >= row_bytes {
                stride
            } else {
                row_bytes
            };
            let needed_src_bytes = if height > 1 {
                src_stride * ((height as usize) - 1) + row_bytes
            } else {
                row_bytes
            };
            let needed_dst_bytes = row_bytes * (height as usize);

            let available_bytes = if chunk_size >= needed_src_bytes {
                chunk_size
            } else if slice.len() >= needed_src_bytes {
                slice.len()
            } else {
                return;
            };

            let raw_bytes = &slice[..available_bytes];
            let video_fmt = ctx.format.format();

            // GPUI's WGPU atlas texture expects BGRA8 data.
            let mut bgra_buf = vec![0u8; needed_dst_bytes];
            let is_rgba_order = matches!(
                video_fmt,
                spa::param::video::VideoFormat::RGBA | spa::param::video::VideoFormat::RGBx
            );

            if !is_rgba_order && src_stride == row_bytes {
                bgra_buf.copy_from_slice(&raw_bytes[..needed_dst_bytes]);
                for pixel in bgra_buf.chunks_exact_mut(4) {
                    pixel[3] = 255;
                }
            } else {
                for y in 0..(height as usize) {
                    let src_row = &raw_bytes[y * src_stride..y * src_stride + row_bytes];
                    let dst_row = &mut bgra_buf[y * row_bytes..(y + 1) * row_bytes];
                    if is_rgba_order {
                        for (dst, src) in
                            dst_row.chunks_exact_mut(4).zip(src_row.chunks_exact(4))
                        {
                            dst[0] = src[2]; // B
                            dst[1] = src[1]; // G
                            dst[2] = src[0]; // R
                            dst[3] = 255;    // A
                        }
                    } else {
                        dst_row.copy_from_slice(src_row);
                        for pixel in dst_row.chunks_exact_mut(4) {
                            pixel[3] = 255;
                        }
                    }
                }
            }

            if let Some(img_buffer) = ImageBuffer::from_raw(width, height, bgra_buf) {
                let render_image =
                    Arc::new(gpui::RenderImage::new(smallvec![Frame::new(img_buffer)]));
                if ctx
                    .event_tx
                    .try_send(CaptureEvent::Frame(CapturedFrame {
                        width,
                        height,
                        render_image,
                    }))
                    .is_ok()
                {
                    ctx.last_frame_sent = now;
                }
            }
        })
        .register()?;

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
            pw::spa::utils::Rectangle {
                width: 1920,
                height: 1080
            },
            pw::spa::utils::Rectangle {
                width: 1,
                height: 1
            },
            pw::spa::utils::Rectangle {
                width: 8192,
                height: 8192
            }
        ),
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            pw::spa::utils::Fraction { num: 60, denom: 1 },
            pw::spa::utils::Fraction { num: 0, denom: 1 },
            pw::spa::utils::Fraction { num: 1000, denom: 1 }
        ),
    );

    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(obj),
    )?
    .0
    .into_inner();

    let mut params = [Pod::from_bytes(&values).context("Failed to create SPA Pod")?];

    stream.connect(
        pw::spa::utils::Direction::Input,
        Some(node_id),
        pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
        &mut params,
    )?;

    mainloop.run();
    Ok(())
}
