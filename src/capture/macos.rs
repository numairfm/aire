use anyhow::Result;
use super::{CaptureSender, WindowCapture};

pub struct MacosWindowCapture;

impl WindowCapture for MacosWindowCapture {
    fn pause(&self) {}
    fn resume(&self) {}
    fn stop(&self) {}
    fn set_target_fps(&self, _fps: u32) {}
}

impl MacosWindowCapture {
    pub fn start(
        _event_tx: CaptureSender,
        _initial_target_fps: u32,
    ) -> Result<Option<(Box<dyn WindowCapture>, Option<(u32, u32)>)>> {
        eprintln!("[MACOS] Initializing ScreenCaptureKit capture seam...");
        Ok(Some((Box::new(Self), Some((1280, 720)))))
    }
}
