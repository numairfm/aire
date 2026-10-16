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

pub fn set_window_resize_constraint(
    aspect_ratio: Option<f32>,
    min_size: Option<(f32, f32)>,
    max_size: Option<(f32, f32)>,
) {
    #[cfg(target_os = "linux")]
    {
        linux::LinuxWindowManager::set_resize_constraint(aspect_ratio, min_size, max_size);
    }
}
