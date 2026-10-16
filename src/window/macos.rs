use anyhow::Result;

pub struct MacosWindowManager;

impl MacosWindowManager {
    pub fn configure(_app_id: &str) -> Result<()> {
        // On macOS, gpui_macos automatically sets native_window.setLevel_(NSFloatingWindowLevel)
        // when window options specify `kind: WindowKind::PopUp` or `kind: WindowKind::Floating`.
        Ok(())
    }
}
