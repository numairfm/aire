#![cfg(any(target_os = "linux", target_os = "freebsd"))]
mod linux;

pub use linux::current_platform;
#[cfg(feature = "wayland")]
pub use linux::set_wayland_resize_constraint;
