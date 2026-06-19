//! Home Assistant updater (cross-platform). Talks to the HA REST API with a
//! long-lived access token — no HA add-on needed. Updates Core/OS/Supervisor/
//! add-ons by calling `update.install` for every `update.*` entity that's pending.

use crate::model::Status;
use crate::orchestrator::Ctx;
use crate::util::run_cmd;
use std::time::Duration;

pub async fn run(ctx: &Ctx) {
    if ctx.ha_url.trim().is_empty() || ctx.ha_token.trim().is_empty() {
        ctx.rep.set(Status::Skipped, "Not configured (set HA URL + token in Settings)", 0);
        return;
    }
    let base = ctx.ha_url.trim().trim_end_matches('/').to_string();
    let auth = format!("Authorization: Bearer {}", ctx.ha_token.trim());

    ctx.rep.set(Status::Running, "Checking Home Assistant…", 20);
    let states = run_cmd(
        "curl",
        &["-s", "-m", "30", "-H", &auth, &format!("{base}/api/states")],
        Duration::from_secs(40),
    )
    .await;

    if !states.stdout.contains("entity_id") {
        ctx.rep.set(Status::Warning, "Could not reach Home Assistant API", 50);
        return;
    }

    // Find every update.* entity that has an update available (state "on").
    let pending: Vec<String> = serde_json::from_str::<serde_json::Value>(&states.stdout)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| {
                    let id = e.get("entity_id")?.as_str()?;
                    let st = e.get("state")?.as_str()?;
                    (id.starts_with("update.") && st == "on").then(|| id.to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    if pending.is_empty() {
        ctx.rep.set(Status::Success, "Home Assistant up to date", 100);
        return;
    }

    let total = pending.len();
    let mut done = 0usize;
    for (i, ent) in pending.iter().enumerate() {
        if ctx.cancelled() {
            break;
        }
        ctx.rep.set(
            Status::Running,
            &format!("Updating {ent}…"),
            20 + ((i as i32) * 70 / total as i32),
        );
        let body = format!("{{\"entity_id\":\"{ent}\"}}");
        let r = run_cmd(
            "curl",
            &[
                "-s", "-m", "900", "-X", "POST", "-H", &auth,
                "-H", "Content-Type: application/json", "-d", &body,
                &format!("{base}/api/services/update/install"),
            ],
            Duration::from_secs(960),
        )
        .await;
        if r.code == Some(0) {
            done += 1;
        }
    }

    if done == total {
        ctx.rep.set(Status::Success, &format!("Updated {done} HA component(s)"), 100);
    } else {
        ctx.rep.set(Status::Warning, &format!("Updated {done}/{total} HA component(s)"), 50);
    }
}
