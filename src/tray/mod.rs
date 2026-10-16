use std::sync::Arc;
use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    SpawnMirror,
    TogglePauseAll,
    CloseAllMirrors,
    OpenSettings,
    Quit,
}

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub mod desktop;

#[derive(Clone)]
pub struct TrayHandle {
    #[cfg(target_os = "linux")]
    inner: Arc<linux::LinuxTrayHandle>,
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    inner: Arc<desktop::DesktopTrayHandle>,
}

impl TrayHandle {
    pub fn update_mirror_state(&self, active_count: usize, all_paused: bool) {
        self.inner.update_mirror_state(active_count, all_paused);
    }
}

pub fn start_system_tray() -> Result<(TrayHandle, async_channel::Receiver<TrayCommand>)> {
    let (tx, rx) = async_channel::bounded::<TrayCommand>(32);

    #[cfg(target_os = "linux")]
    {
        let inner = linux::start_linux_tray(tx)?;
        Ok((
            TrayHandle {
                inner: Arc::new(inner),
            },
            rx,
        ))
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let inner = desktop::start_desktop_tray(tx)?;
        Ok((
            TrayHandle {
                inner: Arc::new(inner),
            },
            rx,
        ))
    }
}

/// Generates a crisp 32x32 custom Picture-in-Picture icon in ARGB32 (network byte order)
/// and RGBA32 formats so the tray icon looks clean and native on all platforms without external asset files.
pub fn generate_tray_icon_argb32(size: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    let s = size as f32;
    for y in 0..size {
        for x in 0..size {
            let fx = x as f32 / s;
            let fy = y as f32 / s;

            // Outer rounded frame (0.10..0.90)
            let in_outer = fx >= 0.09 && fx <= 0.91 && fy >= 0.14 && fy <= 0.86;
            let in_outer_inner = fx >= 0.16 && fx <= 0.84 && fy >= 0.21 && fy <= 0.79;
            let is_outer_border = in_outer && !in_outer_inner;

            // Inner PiP floating card in bottom-right (0.44..0.82, 0.44..0.76)
            let in_pip = fx >= 0.44 && fx <= 0.81 && fy >= 0.44 && fy <= 0.76;

            if in_pip {
                // Emerald accent PiP window (#10b981) -> ARGB
                data.extend_from_slice(&[255, 0x10, 0xb9, 0x81]);
            } else if is_outer_border {
                // Crisp frosty white frame (#f9fafb) -> ARGB
                data.extend_from_slice(&[245, 0xf9, 0xfa, 0xfb]);
            } else if in_outer_inner {
                // Subtle translucent glass interior
                data.extend_from_slice(&[70, 0xe5, 0xe7, 0xeb]);
            } else {
                data.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    data
}

#[allow(dead_code)]
pub fn generate_tray_icon_rgba32(size: u32) -> Vec<u8> {
    let argb = generate_tray_icon_argb32(size);
    let mut rgba = Vec::with_capacity(argb.len());
    for px in argb.chunks_exact(4) {
        rgba.extend_from_slice(&[px[1], px[2], px[3], px[0]]);
    }
    rgba
}
