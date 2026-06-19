//! Per-OS update backends. The orchestrator calls `run(id, ctx)`; each OS
//! compiles only its own module.

use crate::orchestrator::Ctx;

mod homeassistant;
#[cfg(windows)]
mod windows;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod linux;

pub async fn run(id: &str, ctx: &Ctx) {
    // Cross-platform components first.
    if id == "homeassistant" {
        return homeassistant::run(ctx).await;
    }
    #[cfg(windows)]
    windows::run(id, ctx).await;
    #[cfg(target_os = "macos")]
    macos::run(id, ctx).await;
    #[cfg(target_os = "linux")]
    linux::run(id, ctx).await;
}
