use anyhow::Result;

pub struct WindowsWindowManager;

impl WindowsWindowManager {
    pub fn set_always_on_top(_app_id: &str, _enabled: bool) -> Result<()> {
        // On Windows, gpui_windows uses WS_EX_TOOLWINDOW and HWND_TOPMOST for PopUp windows.
        Ok(())
    }
}
