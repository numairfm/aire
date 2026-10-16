use std::{
    os::fd::OwnedFd,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
};

use anyhow::{Context, Result};
use ashpd::{
    desktop::screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType, StartCastOptions},
    enumflags2::BitFlags,
};
use image::{Frame, ImageBuffer};
use pipewire as pw;
use pw::{properties::properties, spa};
use smallvec::smallvec;
use spa::pod::Pod;

use super::{CaptureEvent, CaptureSender, CapturedFrame, WindowCapture};

use std::time::Duration;

pub struct LinuxWindowCapture {
    is_paused: Arc<AtomicBool>,
}

impl LinuxWindowCapture {
    pub fn start(
        event_tx: CaptureSender,
    ) -> Result<Option<(Box<dyn WindowCapture>, Option<(u32, u32)>)>> {
        eprintln!("[PORTAL] Requesting window selection from XDG Desktop Portal...");
        let portal_res = async_io::block_on(async {
            let proxy = Screencast::new().await.context("Failed to connect to ScreenCast portal")?;
            let session = proxy.create_session(Default::default()).await.context("Failed to create screencast session")?;

            let options = SelectSourcesOptions::default()
                .set_cursor_mode(CursorMode::Metadata)
                .set_sources(BitFlags::from_flag(SourceType::Window))
                .set_multiple(false);

            proxy.select_sources(&session, options).await.context("SelectSources failed")?;

            let request = proxy.start(&session, None, StartCastOptions::default()).await?;
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
            let pw_fd = proxy.open_pipe_wire_remote(&session, Default::default()).await.ok();
            Ok::<Option<(u32, Option<OwnedFd>)>, anyhow::Error>(Some((node_id, pw_fd)))
        });

        let (node_id, pw_fd) = match portal_res {
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

        let is_paused_clone = is_paused.clone();
        let tx = event_tx.clone();
        let (dim_tx, dim_rx) = crossbeam_channel::bounded::<(u32, u32)>(1);

        thread::spawn(move || {
            if let Err(err) = run_pipewire_loop(
                node_id,
                pw_fd,
                tx.clone(),
                is_paused_clone,
                is_running,
                Some(dim_tx),
            ) {
                let _ = tx.try_send(CaptureEvent::Error(format!("PipeWire stream error: {}", err)));
            }
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
            Box::new(LinuxWindowCapture { is_paused }),
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
}

struct StreamContext {
    format: spa::param::video::VideoInfoRaw,
    event_tx: CaptureSender,
    initial_dim_tx: Option<crossbeam_channel::Sender<(u32, u32)>>,
    is_paused: Arc<AtomicBool>,
    is_running: Arc<AtomicBool>,
    last_width: u32,
    last_height: u32,
}

fn run_pipewire_loop(
    node_id: u32,
    stream_fd: Option<OwnedFd>,
    event_tx: CaptureSender,
    is_paused: Arc<AtomicBool>,
    is_running: Arc<AtomicBool>,
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

    let stream_ctx = StreamContext {
        format: Default::default(),
        event_tx: event_tx.clone(),
        initial_dim_tx,
        is_paused,
        is_running: is_running.clone(),
        last_width: 0,
        last_height: 0,
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
            if matches!(new, pw::stream::StreamState::Unconnected | pw::stream::StreamState::Error(_)) {
                let _ = ctx.event_tx.try_send(CaptureEvent::SourceClosed);
            }
        })
        .param_changed(|_, ctx, id, param| {
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
            if !ctx.is_running.load(Ordering::Relaxed) {
                return;
            }

            if ctx.is_paused.load(Ordering::Relaxed) {
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

            let width = ctx.format.size().width;
            let height = ctx.format.size().height;
            if width == 0 || height == 0 {
                return;
            }

            let data = &mut datas[0];
            let chunk_size = data.chunk().size() as usize;
            let Some(slice) = data.data() else {
                return;
            };

            let needed_bytes = (width * height * 4) as usize;
            if slice.len() < chunk_size || chunk_size < needed_bytes {
                return;
            }

            let raw_bytes = &slice[..chunk_size];
            let is_bgr = match ctx.format.format() {
                spa::param::video::VideoFormat::BGRx | spa::param::video::VideoFormat::BGRA => true,
                _ => false,
            };

            let mut rgba_buf = vec![0u8; needed_bytes];
            if is_bgr {
                for i in (0..needed_bytes).step_by(4) {
                    rgba_buf[i] = raw_bytes[i + 2];     // R
                    rgba_buf[i + 1] = raw_bytes[i + 1]; // G
                    rgba_buf[i + 2] = raw_bytes[i];     // B
                    rgba_buf[i + 3] = 255;              // A
                }
            } else {
                rgba_buf.copy_from_slice(&raw_bytes[..needed_bytes]);
            }

            if let Some(img_buffer) = ImageBuffer::from_raw(width, height, rgba_buf) {
                let render_image = Arc::new(gpui::RenderImage::new(smallvec![Frame::new(img_buffer)]));
                let _ = ctx.event_tx.try_send(CaptureEvent::Frame(CapturedFrame {
                    width,
                    height,
                    render_image,
                }));
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

    mainloop.run();
    Ok(())
}
