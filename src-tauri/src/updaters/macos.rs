//! macOS updaters. Upgrade-only: brew/mas act on what's installed;
//! softwareupdate installs pending OS/security updates via an admin prompt.

use crate::model::Status;
use crate::orchestrator::Ctx;
use crate::util::run_cmd;
use std::time::Duration;

pub async fn run(id: &str, ctx: &Ctx) {
    match id {
        "macos-update" => macos_update(ctx).await,
        "brew" => brew(ctx).await,
        "mas" => mas(ctx).await,
        other => ctx.rep.set(Status::Skipped, &format!("Unknown component '{other}'"), 0),
    }
}

/// Dry-run: report available updates without applying.
pub async fn check(id: &str, ctx: &Ctx) {
    match id {
        "macos-update" => macos_update_check(ctx).await,
        "brew" => brew_check(ctx).await,
        "mas" => mas_check(ctx).await,
        _ => super::no_check(ctx),
    }
}

async fn macos_update_check(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Checking for macOS updates…", 40);
    let res = run_cmd("softwareupdate", &["-l"], Duration::from_secs(300)).await;
    let out = res.combined();
    if out.contains("No new software available") {
        return super::report_count(ctx, 0, "update");
    }
    // Each available update is a "* Label: …" / "* Title:" line.
    let n = out.lines().filter(|l| l.trim_start().starts_with('*')).count();
    super::report_count(ctx, n, "update");
}

async fn brew_check(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Checking Homebrew for outdated formulae…", 40);
    let res = run_cmd("brew", &["outdated", "--quiet"], Duration::from_secs(300)).await;
    let n = res.stdout.lines().filter(|l| !l.trim().is_empty()).count();
    super::report_count(ctx, n, "package");
}

async fn mas_check(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Checking the Mac App Store…", 40);
    let res = run_cmd("mas", &["outdated"], Duration::from_secs(300)).await;
    let n = res.stdout.lines().filter(|l| !l.trim().is_empty()).count();
    super::report_count(ctx, n, "app");
}

// ---- macOS Software Update (needs root → AppleScript admin prompt) ----
async fn macos_update(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Installing macOS updates (admin prompt)…", 30);
    let res = run_cmd(
        "osascript",
        &[
            "-e",
            "do shell script \"softwareupdate -ia\" with administrator privileges",
        ],
        Duration::from_secs(3600),
    )
    .await;
    if res.code == Some(0) {
        ctx.rep.set(Status::Success, "macOS updates installed", 100);
    } else if res.combined().contains("User canceled") {
        ctx.rep.set(Status::Skipped, "Admin prompt cancelled", 0);
    } else {
        ctx.rep.set(Status::Warning, &format!("Exit: {:?}", res.code), 50);
    }
}

// ---- Homebrew (user-level, no root) ----
async fn brew(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "brew update…", 20);
    run_cmd("brew", &["update"], Duration::from_secs(300)).await;
    if ctx.cancelled() {
        return;
    }
    ctx.rep.set(Status::Running, "brew upgrade…", 60);
    let res = run_cmd("brew", &["upgrade"], Duration::from_secs(2400)).await;
    if res.code == Some(0) {
        ctx.rep.set(Status::Success, "Homebrew packages upgraded", 100);
    } else {
        ctx.rep.set(Status::Warning, &format!("Exit: {:?}", res.code), 50);
    }
}

// ---- Mac App Store (mas) ----
async fn mas(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Updating Mac App Store apps…", 40);
    let res = run_cmd("mas", &["upgrade"], Duration::from_secs(2400)).await;
    if res.code == Some(0) {
        ctx.rep.set(Status::Success, "App Store apps updated", 100);
    } else {
        ctx.rep.set(Status::Warning, &format!("Exit: {:?}", res.code), 50);
    }
}
