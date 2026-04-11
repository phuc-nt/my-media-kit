//! App metadata commands. Cheap, no state — safe to call from the very
//! first paint of the UI.

use serde::Serialize;
use tauri::command;

#[command]
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[derive(Debug, Clone, Serialize)]
pub struct PlatformInfo {
    pub os: &'static str,
    pub arch: &'static str,
    pub supports_mlx: bool,
    pub supports_apple_intelligence: bool,
}

#[command]
pub fn platform_info() -> PlatformInfo {
    let supports_mlx = cfg!(all(target_os = "macos", target_arch = "aarch64"));
    let supports_apple_intelligence = supports_mlx;
    PlatformInfo {
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        supports_mlx,
        supports_apple_intelligence,
    }
}
