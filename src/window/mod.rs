use anyhow::Result;


#[cfg(target_os = "linux")]
pub mod linux;

pub fn configure_platform_window(app_id: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::LinuxWindowManager::configure(app_id)
    }

    #[cfg(not(target_os = "linux"))]
    {
        Ok(())
    }
}
