//! Cross-platform dev-tool updaters (real executables, safe on every OS).

use crate::model::Status;
use crate::orchestrator::Ctx;
use crate::util::run_cmd;
use std::time::Duration;

pub async fn run(id: &str, ctx: &Ctx) {
    match id {
        "rustup" => simple(ctx, "rustup", &["update"], "Rust toolchains updated").await,
        "dotnet-tools" => {
            simple(
                ctx,
                "dotnet",
                &["tool", "update", "--all", "--global"],
                ".NET global tools updated",
            )
            .await
        }
        other => ctx.rep.set(Status::Skipped, &format!("Unknown component '{other}'"), 0),
    }
}

async fn simple(ctx: &Ctx, program: &str, args: &[&str], ok: &str) {
    ctx.rep.set(Status::Running, &format!("{program} {}…", args.join(" ")), 40);
    let res = run_cmd(program, args, Duration::from_secs(1800)).await;
    if res.code == Some(0) {
        ctx.rep.set(Status::Success, ok, 100);
    } else {
        ctx.rep.set(Status::Warning, &format!("Exit: {:?}", res.code), 50);
    }
}
