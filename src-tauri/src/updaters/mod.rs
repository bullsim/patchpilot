//! Per-OS update backends. The orchestrator calls `run(id, ctx)`; each OS
//! compiles only its own module.

use crate::model::Status;
use crate::orchestrator::Ctx;
use crate::util::{run_cmd, CmdResult};
use std::time::Duration;

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

/// Run a full command line through the OS shell. On Windows this resolves the
/// `.cmd` shims (npm, pip launchers); on Unix it goes through `sh -c`.
pub(crate) async fn run_shell(cmdline: &str, secs: u64) -> CmdResult {
    #[cfg(windows)]
    {
        run_cmd("cmd", &["/C", cmdline], Duration::from_secs(secs)).await
    }
    #[cfg(not(windows))]
    {
        run_cmd("sh", &["-c", cmdline], Duration::from_secs(secs)).await
    }
}

/// The working pip invocation prefix for this machine, or None if pip is absent.
/// Tries `python -m pip` then `python3 -m pip` so it works without a bare `pip`.
pub(crate) async fn pip_prefix() -> Option<&'static str> {
    for prefix in ["python -m pip", "python3 -m pip"] {
        if run_shell(&format!("{prefix} --version"), 15).await.code == Some(0) {
            return Some(prefix);
        }
    }
    None
}
