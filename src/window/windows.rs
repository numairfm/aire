use anyhow::Result;

pub struct WindowsWindowManager;

impl WindowsWindowManager {
    pub fn configure(_app_id: &str) -> Result<()> {
        // On Windows, gpui_windows uses WS_EX_TOOLWINDOW and HWND_TOPMOST
        // for PopUp/Floating window kinds.
        Ok(())
    }
}
