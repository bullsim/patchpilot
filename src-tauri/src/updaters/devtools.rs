//! Cross-platform dev-tool updaters (real executables, safe on every OS).

use crate::model::Status;
use crate::orchestrator::Ctx;
use crate::util::run_cmd;
use std::time::Duration;

pub async fn run(id: &str, ctx: &Ctx) {
    match id {
        "rustup" => rustup(ctx).await,
        "dotnet-tools" => dotnet(ctx).await,
        "npm-global" => npm(ctx).await,
        "pip" => pip(ctx).await,
        other => ctx.rep.set(Status::Skipped, &format!("Unknown component '{other}'"), 0),
    }
}

/// Dry-run: report whether toolchain updates are available.
pub async fn check(id: &str, ctx: &Ctx) {
    match id {
        "rustup" => rustup_check(ctx).await,
        "npm-global" => npm_check(ctx).await,
        "pip" => pip_check(ctx).await,
        _ => super::no_check(ctx),
    }
}

// ---- npm global packages ----
async fn npm(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Updating global npm packages…", 40);
    let res = super::run_shell("npm update -g", 1800).await;
    if res.code == Some(0) {
        ctx.rep.set(Status::Success, "Global npm packages updated", 100);
    } else {
        ctx.rep.set(Status::Warning, &format!("npm exit: {:?}", res.code), 50);
    }
}

async fn npm_check(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Checking global npm packages…", 40);
    // `npm outdated -g --parseable` prints one line per outdated package (exit 1 if any).
    let res = super::run_shell("npm outdated -g --parseable", 180).await;
    let n = res.stdout.lines().filter(|l| !l.trim().is_empty()).count();
    super::report_count(ctx, n, "package");
}

// ---- pip (user/global Python packages) ----
async fn pip(ctx: &Ctx) {
    let Some(prefix) = super::pip_prefix().await else {
        ctx.rep.set(Status::Skipped, "pip not found", 0);
        return;
    };
    ctx.rep.set(Status::Running, "Finding outdated Python packages…", 30);
    let names = pip_outdated_names(prefix).await;
    if names.is_empty() {
        ctx.rep.set(Status::Success, "Python packages up to date", 100);
        return;
    }
    let total = names.len();
    ctx.rep.set(Status::Running, &format!("Upgrading {total} package(s)…"), 60);
    let res = super::run_shell(&format!("{prefix} install -U {}", names.join(" ")), 2400).await;
    if res.code == Some(0) {
        ctx.rep.set(Status::Success, &format!("Upgraded {total} Python package(s)"), 100);
    } else {
        ctx.rep.set(Status::Warning, &format!("pip exit: {:?}", res.code), 50);
    }
}

async fn pip_check(ctx: &Ctx) {
    let Some(prefix) = super::pip_prefix().await else {
        ctx.rep.set(Status::Skipped, "pip not found", 0);
        return;
    };
    ctx.rep.set(Status::Running, "Checking Python packages…", 40);
    super::report_count(ctx, pip_outdated_names(prefix).await.len(), "package");
}

/// Names of outdated pip packages via `pip list --outdated --format=json`.
async fn pip_outdated_names(prefix: &str) -> Vec<String> {
    let res = super::run_shell(&format!("{prefix} list --outdated --format=json"), 180).await;
    serde_json::from_str::<serde_json::Value>(res.stdout.trim())
        .ok()
        .and_then(|v| v.as_array().cloned())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.get("name").and_then(|n| n.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

async fn rustup_check(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Checking rustup toolchains…", 40);
    let res = run_cmd("rustup", &["check"], Duration::from_secs(180)).await;
    // `rustup check` prints "Update available" per toolchain, or "Up to date".
    let n = res
        .stdout
        .lines()
        .filter(|l| l.contains("Update available"))
        .count();
    super::report_count(ctx, n, "toolchain");
}

async fn rustup(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "rustup update…", 40);
    let res = run_cmd("rustup", &["update"], Duration::from_secs(1800)).await;
    if res.code == Some(0) {
        ctx.rep.set(Status::Success, "Rust toolchains updated", 100);
    } else {
        ctx.rep.set(Status::Warning, &format!("Exit: {:?}", res.code), 50);
    }
}

// .NET: update the runtimes/SDKs (Windows, via winget) + global tools (all OSes), silently.
async fn dotnet(ctx: &Ctx) {
    #[cfg(windows)]
    dotnet_runtimes_winget(ctx).await;
    dotnet_tools(ctx).await;
}

// Silently upgrade installed .NET SDK/runtime winget packages. winget skips ids that
// aren't installed, so the known-id sweep is safe (no parsing of truncated tables).
#[cfg(windows)]
async fn dotnet_runtimes_winget(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Updating .NET runtimes/SDKs…", 10);
    let flags = [
        "--silent",
        "--disable-interactivity",
        "--accept-source-agreements",
        "--accept-package-agreements",
    ];
    let families = ["SDK", "Runtime", "DesktopRuntime", "AspNetCore"];
    for v in ["6", "7", "8", "9", "10"] {
        for fam in families {
            if ctx.cancelled() {
                return;
            }
            let id = format!("Microsoft.DotNet.{fam}.{v}");
            let mut args = vec!["upgrade", "--id", id.as_str(), "--exact"];
            args.extend_from_slice(&flags);
            run_cmd("winget", &args, Duration::from_secs(900)).await;
        }
    }
}

// .NET global tools: update each silently (no --all dependency; graceful skips).
async fn dotnet_tools(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Updating .NET global tools…", 60);
    let list = run_cmd("dotnet", &["tool", "list", "--global"], Duration::from_secs(60)).await;
    if list.code != Some(0) {
        // Runtimes may still have been updated above; don't hard-fail.
        ctx.rep.set(Status::Success, ".NET runtimes checked (no tool SDK)", 100);
        return;
    }
    let ids: Vec<String> = list
        .stdout
        .lines()
        .skip(2)
        .filter_map(|l| l.split_whitespace().next())
        .filter(|s| !s.is_empty() && !s.starts_with('-'))
        .map(str::to_string)
        .collect();

    if ids.is_empty() {
        ctx.rep.set(Status::Success, ".NET runtimes checked; no global tools", 100);
        return;
    }

    let total = ids.len();
    let mut ok = 0usize;
    for (i, id) in ids.iter().enumerate() {
        if ctx.cancelled() {
            break;
        }
        ctx.rep.set(Status::Running, &format!("Updating {id}…"), 70 + ((i as i32) * 25 / total as i32));
        let r = run_cmd(
            "dotnet",
            &["tool", "update", id, "--global", "--verbosity", "quiet"],
            Duration::from_secs(600),
        )
        .await;
        if r.code == Some(0) {
            ok += 1;
        }
    }

    if ok == total {
        ctx.rep.set(Status::Success, &format!(".NET updated ({ok} tool(s))"), 100);
    } else {
        ctx.rep.set(Status::Warning, &format!(".NET tools {ok}/{total} updated"), 60);
    }
}
