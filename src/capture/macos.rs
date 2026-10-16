use anyhow::Result;
use super::{CaptureSender, WindowCapture};

pub struct MacosWindowCapture;

impl MacosWindowCapture {
    pub fn start(
        _event_tx: CaptureSender,
        _initial_target_fps: u32,
    ) -> Result<Option<(Box<dyn WindowCapture>, Option<(u32, u32)>)>> {
        anyhow::bail!("macOS ScreenCaptureKit backend is not yet enabled on this build")
    }
}
