use anyhow::Result;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

pub fn configure_platform_window(app_id: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::LinuxWindowManager::configure(app_id)
    }

    #[cfg(target_os = "macos")]
    {
        macos::MacosWindowManager::configure(app_id)
    }

    #[cfg(target_os = "windows")]
    {
        windows::WindowsWindowManager::configure(app_id)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Ok(())
    }
}

pub fn set_window_resize_constraint(
    aspect_ratio: Option<f32>,
    min_size: Option<(f32, f32)>,
    max_size: Option<(f32, f32)>,
) {
    #[cfg(target_os = "linux")]
    {
        linux::LinuxWindowManager::set_resize_constraint(aspect_ratio, min_size, max_size);
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (aspect_ratio, min_size, max_size);
    }
}
