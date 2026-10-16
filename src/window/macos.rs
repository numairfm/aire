use anyhow::Result;

pub struct MacosWindowManager;

impl MacosWindowManager {
    pub fn set_always_on_top(_app_id: &str, _enabled: bool) -> Result<()> {
        // On macOS, gpui_macos sets NSFloatingWindowLevel for WindowKind::PopUp
        // and NSNormalWindowLevel for WindowKind::Normal.
        Ok(())
    }
}
