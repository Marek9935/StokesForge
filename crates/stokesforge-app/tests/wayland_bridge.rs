//! Guard #72: the shared receiver channel and its lifetime, without a compositor.

#![cfg(target_os = "linux")]
#[path = "../../../vendor/winit/src/platform_impl/linux/wayland/file_drop_bridge.rs"]
mod bridge;
