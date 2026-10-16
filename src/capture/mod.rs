use std::sync::Arc;
use anyhow::Result;
use gpui::RenderImage;

#[derive(Clone)]
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    pub render_image: Arc<RenderImage>,
}

#[derive(Clone)]
pub enum CaptureEvent {
    Frame(CapturedFrame),
    Resized { width: u32, height: u32 },
    SourceClosed,
    Error(String),
}

pub type CaptureSender = async_channel::Sender<CaptureEvent>;
pub type CaptureReceiver = async_channel::Receiver<CaptureEvent>;

pub fn create_capture_channel() -> (CaptureSender, CaptureReceiver) {
    async_channel::bounded::<CaptureEvent>(2)
}

pub trait WindowCapture: Send + Sync {
    fn pause(&self);
    fn resume(&self);
}

#[cfg(target_os = "linux")]
pub mod linux;

pub fn start_window_capture(
    event_tx: CaptureSender,
) -> Result<Option<(Box<dyn WindowCapture>, Option<(u32, u32)>)>> {
    #[cfg(target_os = "linux")]
    {
        linux::LinuxWindowCapture::start(event_tx)
    }

    #[cfg(not(target_os = "linux"))]
    {
        anyhow::bail!("Window capture is currently only implemented for Linux");
    }
}
