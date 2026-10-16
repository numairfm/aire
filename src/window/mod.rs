use anyhow::Result;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

pub fn configure_platform_window(app_id: &str, always_on_top: bool) -> Result<()> {
    set_always_on_top(app_id, always_on_top)
}

pub fn set_always_on_top(app_id: &str, enabled: bool) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::LinuxWindowManager::set_always_on_top(app_id, enabled)
    }

    #[cfg(target_os = "macos")]
    {
        macos::MacosWindowManager::set_always_on_top(app_id, enabled)
    }

    #[cfg(target_os = "windows")]
    {
        windows::WindowsWindowManager::set_always_on_top(app_id, enabled)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = (app_id, enabled);
        Ok(())
    }
}

pub fn set_window_resize_constraint(
    aspect_ratio: Option<f32>,
    min_size: Option<(f32, f32)>,
    max_size: Option<(f32, f32)>,
) {
    set_window_resize_constraint_for(None, aspect_ratio, min_size, max_size);
}

pub fn set_window_resize_constraint_for(
    window_id: Option<u64>,
    aspect_ratio: Option<f32>,
    min_size: Option<(f32, f32)>,
    max_size: Option<(f32, f32)>,
) {
    #[cfg(target_os = "linux")]
    {
        linux::LinuxWindowManager::set_resize_constraint_for(
            window_id,
            aspect_ratio,
            min_size,
            max_size,
        );
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (window_id, aspect_ratio, min_size, max_size);
    }
}

pub fn remove_window_resize_constraint(window_id: u64) {
    #[cfg(target_os = "linux")]
    {
        linux::LinuxWindowManager::remove_resize_constraint(window_id);
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = window_id;
    }
}
