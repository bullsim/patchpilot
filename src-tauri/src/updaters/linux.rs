//! Linux updaters (Debian/Zorin/Raspberry Pi OS). Upgrade-only.
//! Privileged steps use pkexec (GUI polkit prompt); user-level ones run directly.

use crate::model::Status;
use crate::orchestrator::Ctx;
use crate::util::{run_cmd, CmdResult};
use std::time::Duration;

pub async fn run(id: &str, ctx: &Ctx) {
    match id {
        "apt" => apt(ctx).await,
        "flatpak" => flatpak(ctx).await,
        "snap" => snap(ctx).await,
        "fwupd" => fwupd(ctx).await,
        other => ctx.rep.set(Status::Skipped, &format!("Unknown component '{other}'"), 0),
    }
}

/// Best short reason from a failed command (last non-empty stderr/stdout line).
fn reason(res: &CmdResult) -> String {
    let pick = |s: &str| {
        s.lines()
            .rev()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(|l| l.chars().take(140).collect::<String>())
    };
    pick(&res.stderr)
        .or_else(|| pick(&res.stdout))
        .unwrap_or_else(|| format!("exit {:?}", res.code))
}

// ---- APT packages (needs root via pkexec) ----
async fn apt(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "apt update + upgrade (admin prompt)…", 30);
    let res = run_cmd(
        "pkexec",
        &[
            "sh",
            "-c",
            "apt-get update && DEBIAN_FRONTEND=noninteractive apt-get -y upgrade",
        ],
        Duration::from_secs(3600),
    )
    .await;
    match res.code {
        Some(0) => ctx.rep.set(Status::Success, "APT packages upgraded", 100),
        Some(126) | Some(127) => {
            ctx.rep.set(Status::Skipped, "Admin prompt cancelled / not authorised", 0)
        }
        Some(100) => ctx.rep.set(
            Status::Warning,
            "apt is busy (another updater holds the lock) — try again shortly",
            50,
        ),
        None if res.timed_out => ctx.rep.set(Status::Warning, "Timed out", 50),
        None => ctx.rep.set(
            Status::Warning,
            "Couldn't run pkexec — install policykit-1, or run PatchPilot from a terminal",
            50,
        ),
        _ => ctx.rep.set(Status::Warning, &format!("apt: {}", reason(&res)), 50),
    }
}

// ---- Flatpak (user/system installed apps) ----
async fn flatpak(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "flatpak update…", 40);
    let res = run_cmd("flatpak", &["update", "-y"], Duration::from_secs(2400)).await;
    match res.code {
        Some(0) => ctx.rep.set(Status::Success, "Flatpaks updated", 100),
        _ => ctx.rep.set(Status::Warning, &format!("flatpak: {}", reason(&res)), 50),
    }
}

// ---- Snap (needs root via pkexec) ----
async fn snap(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "snap refresh (admin prompt)…", 40);
    let res = run_cmd("pkexec", &["snap", "refresh"], Duration::from_secs(2400)).await;
    match res.code {
        Some(0) => ctx.rep.set(Status::Success, "Snaps refreshed", 100),
        Some(126) | Some(127) => {
            ctx.rep.set(Status::Skipped, "Admin prompt cancelled / not authorised", 0)
        }
        None => ctx.rep.set(Status::Warning, "Couldn't run pkexec (install policykit-1)", 50),
        _ => ctx.rep.set(Status::Warning, &format!("snap: {}", reason(&res)), 50),
    }
}

// ---- Firmware via fwupd (LVFS) ----
async fn fwupd(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Refreshing firmware metadata…", 30);
    run_cmd("fwupdmgr", &["refresh", "--force"], Duration::from_secs(180)).await;
    if ctx.cancelled() {
        return;
    }
    ctx.rep.set(Status::Running, "Applying firmware updates…", 70);
    let res = run_cmd("fwupdmgr", &["update", "-y"], Duration::from_secs(3600)).await;
    match res.code {
        Some(0) => ctx.rep.set(Status::Success, "Firmware up to date", 100),
        // fwupd returns 2 when there is nothing to do.
        Some(2) => ctx.rep.set(Status::Success, "No firmware updates", 100),
        _ => ctx.rep.set(Status::Warning, &format!("fwupd: {}", reason(&res)), 50),
    }
}
