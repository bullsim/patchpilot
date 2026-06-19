use crate::paths::reboot_path;
use crate::util::run_cmd;
use chrono::{Datelike, Duration as ChronoDur, Local, NaiveTime, TimeZone, Timelike};
use std::time::Duration;

const TASK_NAME: &str = "PatchPilot_PendingReboot";
const SHUTDOWN_ARGS: &str =
    r#"shutdown.exe /r /t 60 /c "Firmware reboot scheduled by PatchPilot""#;

/// Schedule (or perform) a reboot. `when` is "now" or "HH:mm".
/// Returns the ISO 8601 datetime it is scheduled for ("now" -> immediate).
pub async fn schedule(when: &str) -> Result<String, String> {
    let when = when.trim();

    if when.eq_ignore_ascii_case("now") {
        run_cmd(
            "shutdown",
            &["/r", "/t", "60", "/c", "Firmware reboot - PatchPilot"],
            Duration::from_secs(15),
        )
        .await;
        return Ok(Local::now().to_rfc3339());
    }

    let time = NaiveTime::parse_from_str(when, "%H:%M")
        .map_err(|_| format!("invalid time '{when}', expected HH:mm"))?;

    // Next occurrence of that time (today if still in the future, else tomorrow).
    let now = Local::now();
    let mut target = Local
        .with_ymd_and_hms(now.year(), now.month(), now.day(), time.hour(), time.minute(), 0)
        .single()
        .ok_or("could not build target time")?;
    if target <= now {
        target += ChronoDur::days(1);
    }

    if cfg!(windows) {
        let st = target.format("%H:%M").to_string();
        let sd = target.format("%m/%d/%Y").to_string();
        let res = run_cmd(
            "schtasks",
            &[
                "/Create", "/TN", TASK_NAME, "/TR", SHUTDOWN_ARGS, "/SC", "ONCE", "/ST", &st,
                "/SD", &sd, "/RL", "HIGHEST", "/F",
            ],
            Duration::from_secs(20),
        )
        .await;
        if !crate::util::is_winget_ok(res.code) && res.code != Some(0) {
            return Err(format!("schtasks failed: {}", res.combined()));
        }
    }

    let iso = target.to_rfc3339();
    let _ = std::fs::write(reboot_path(), &iso);
    Ok(iso)
}

pub async fn cancel() {
    if cfg!(windows) {
        run_cmd(
            "schtasks",
            &["/Delete", "/TN", TASK_NAME, "/F"],
            Duration::from_secs(20),
        )
        .await;
    }
    let _ = std::fs::remove_file(reboot_path());
}

/// Returns the ISO datetime of a pending reboot, if the task still exists.
pub async fn pending() -> Option<String> {
    let iso = std::fs::read_to_string(reboot_path()).ok()?;
    let iso = iso.trim().to_string();
    if iso.is_empty() {
        return None;
    }
    if cfg!(windows) {
        let res = run_cmd(
            "schtasks",
            &["/Query", "/TN", TASK_NAME],
            Duration::from_secs(15),
        )
        .await;
        if res.code != Some(0) {
            let _ = std::fs::remove_file(reboot_path());
            return None;
        }
    }
    Some(iso)
}
