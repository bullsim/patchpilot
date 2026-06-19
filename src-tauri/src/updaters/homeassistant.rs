//! Home Assistant updater (cross-platform). Talks to the HA REST API with a
//! long-lived access token — no HA add-on needed. Triggers `update.install` for
//! every pending `update.*` entity, ordered so the restart-causing ones go last.

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

    ctx.rep.set(Status::Running, "Checking Home Assistant…", 15);
    let states = run_cmd(
        "curl",
        &["-s", "-m", "30", "-H", &auth, &format!("{base}/api/states")],
        Duration::from_secs(40),
    )
    .await;

    if !states.stdout.contains("entity_id") {
        ctx.rep.set(Status::Warning, "Could not reach HA API (check URL/token)", 50);
        return;
    }

    // Pending updates (state == "on").
    let mut pending: Vec<String> = serde_json::from_str::<serde_json::Value>(&states.stdout)
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

    // Order: add-ons (0) → Supervisor (1) → Core (2) → OS (3). Core/OS restart HA,
    // so doing them last means the earlier installs actually get sent first.
    pending.sort_by_key(|id| {
        if id.contains("operating_system") {
            3
        } else if id.contains("core") {
            2
        } else if id.contains("supervisor") {
            1
        } else {
            0
        }
    });

    let total = pending.len();
    let mut triggered = 0usize;
    for (i, ent) in pending.iter().enumerate() {
        if ctx.cancelled() {
            break;
        }
        let short = ent.strip_prefix("update.").unwrap_or(ent);
        ctx.rep.set(Status::Running, &format!("Updating {short}…"), 20 + ((i as i32) * 70 / total as i32));
        let body = format!("{{\"entity_id\":\"{ent}\"}}");
        let r = run_cmd(
            "curl",
            &[
                "-s", "-o", "/dev/null", "-w", "%{http_code}", "-m", "120",
                "-X", "POST", "-H", &auth, "-H", "Content-Type: application/json",
                "-d", &body, &format!("{base}/api/services/update/install"),
            ],
            Duration::from_secs(130),
        )
        .await;
        // 2xx = accepted; a dropped connection (curl error) usually means HA is
        // already restarting to apply a Core/OS update — also counts as triggered.
        let http_ok = r.stdout.trim().starts_with('2');
        let restarting = r.code != Some(0) && (ent.contains("core") || ent.contains("operating_system"));
        if http_ok || restarting {
            triggered += 1;
        }
    }

    if triggered == total {
        ctx.rep.set(
            Status::Success,
            &format!("Triggered {triggered} update(s) — HA finishes in the background"),
            100,
        );
    } else if triggered > 0 {
        ctx.rep.set(
            Status::Warning,
            &format!("Triggered {triggered}/{total}; some may need a retry after restart"),
            60,
        );
    } else {
        ctx.rep.set(Status::Warning, "Updates found but none accepted (check token scope)", 50);
    }
}
