//! Cross-platform dev-tool updaters (real executables, safe on every OS).

use crate::model::Status;
use crate::orchestrator::Ctx;
use crate::util::run_cmd;
use std::time::Duration;

pub async fn run(id: &str, ctx: &Ctx) {
    match id {
        "rustup" => rustup(ctx).await,
        "dotnet-tools" => dotnet_tools(ctx).await,
        other => ctx.rep.set(Status::Skipped, &format!("Unknown component '{other}'"), 0),
    }
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

// .NET global tools: update each one silently. Avoids the `--all` flag (newer SDKs only)
// and handles "no SDK" / "no tools" gracefully.
async fn dotnet_tools(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Listing .NET global tools…", 20);
    let list = run_cmd("dotnet", &["tool", "list", "--global"], Duration::from_secs(60)).await;
    if list.code != Some(0) {
        ctx.rep.set(Status::Skipped, "dotnet tool management unavailable", 0);
        return;
    }
    // Table: header, separator line, then "<PackageId>  <Version>  <Commands>".
    let ids: Vec<String> = list
        .stdout
        .lines()
        .skip(2)
        .filter_map(|l| l.split_whitespace().next())
        .filter(|s| !s.is_empty() && !s.starts_with('-'))
        .map(str::to_string)
        .collect();

    if ids.is_empty() {
        ctx.rep.set(Status::Skipped, "No global .NET tools installed", 0);
        return;
    }

    let total = ids.len();
    let mut ok = 0usize;
    for (i, id) in ids.iter().enumerate() {
        if ctx.cancelled() {
            break;
        }
        ctx.rep.set(Status::Running, &format!("Updating {id}…"), 30 + ((i as i32) * 65 / total as i32));
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
        ctx.rep.set(Status::Success, &format!("{ok} .NET tool(s) updated"), 100);
    } else {
        ctx.rep.set(Status::Warning, &format!("Updated {ok}/{total} .NET tool(s)"), 60);
    }
}
