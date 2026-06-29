//! Per-OS update backends. The orchestrator calls `run(id, ctx)`; each OS
//! compiles only its own module.

use crate::model::Status;
use crate::orchestrator::Ctx;

mod homeassistant;
mod devtools;
#[cfg(windows)]
mod windows;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod linux;

pub async fn run(id: &str, ctx: &Ctx) {
    // Dry-run: report what would update, change nothing.
    if ctx.check_only {
        return check(id, ctx).await;
    }
    // Cross-platform components first.
    if id == "homeassistant" {
        return homeassistant::run(ctx).await;
    }
    if matches!(id, "rustup" | "dotnet-tools") {
        return devtools::run(id, ctx).await;
    }
    #[cfg(windows)]
    windows::run(id, ctx).await;
    #[cfg(target_os = "macos")]
    macos::run(id, ctx).await;
    #[cfg(target_os = "linux")]
    linux::run(id, ctx).await;
}

/// Dry-run dispatch: report available updates without applying anything.
async fn check(id: &str, ctx: &Ctx) {
    if id == "homeassistant" {
        return homeassistant::check(ctx).await;
    }
    if matches!(id, "rustup" | "dotnet-tools") {
        return devtools::check(id, ctx).await;
    }
    #[cfg(windows)]
    windows::check(id, ctx).await;
    #[cfg(target_os = "macos")]
    macos::check(id, ctx).await;
    #[cfg(target_os = "linux")]
    linux::check(id, ctx).await;
}

/// Report a dry-run result: "N available" (Warning, draws the eye) or "Up to date".
pub(crate) fn report_count(ctx: &Ctx, n: usize, noun: &str) {
    if n == 0 {
        ctx.rep.set(Status::Success, "Up to date", 100);
    } else {
        let plural = if n == 1 { "" } else { "s" };
        ctx.rep.set(Status::Warning, &format!("{n} {noun}{plural} available"), 100);
    }
}

/// Dry-run fallback for components with no safe, non-destructive check.
pub(crate) fn no_check(ctx: &Ctx) {
    ctx.rep.set(Status::Skipped, "No check available — run to update", 100);
}
