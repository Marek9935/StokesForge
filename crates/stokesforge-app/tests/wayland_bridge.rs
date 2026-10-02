//! Garde #72 : le canal du récepteur partagé et sa durée de vie, sans compositeur.

#![cfg(target_os = "linux")]
#[path = "../../../vendor/winit/src/platform_impl/linux/wayland/file_drop_bridge.rs"]
mod bridge;
