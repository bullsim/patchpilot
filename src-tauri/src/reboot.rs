use crate::paths::reboot_path;
use crate::util::run_cmd;
use chrono::{DateTime, Datelike, Duration as ChronoDur, Local, NaiveTime, TimeZone, Timelike};
use std::time::Duration;

const TASK_NAME: &str = "PatchPilot_PendingReboot";

/// A pending reboot whose scheduled time is further in the past than this is
/// considered stale (it fired, was missed, or was never going to fire) and is
/// cleared instead of being shown forever.
const STALE_AFTER_HOURS: i64 = 2;

/// Registers the one-shot reboot task via the Task Scheduler cmdlets.
///
/// Why not `schtasks /SD`: it parses the date in the machine's *locale* short
/// date format, so "08/12/2026" meant 12 Aug on a US machine but 8 Dec on a UK
/// one — the reboot silently never happened. Here the time is passed as an ISO
/// string and parsed with the invariant culture, so it is unambiguous.
///
/// `__AT__` is replaced with `yyyy-MM-ddTHH:mm:ss` (local time).
#[cfg(windows)]
const REGISTER_PS: &str = r#"
$ErrorActionPreference = 'Stop'
$t  = [datetime]::ParseExact('__AT__', 'yyyy-MM-ddTHH:mm:ss', [Globalization.CultureInfo]::InvariantCulture)
$a  = New-ScheduledTaskAction -Execute 'shutdown.exe' -Argument '/r /t 60 /c "Firmware reboot scheduled by PatchPilot"'
$tr = New-ScheduledTaskTrigger -Once -At $t
# Expire 2h after the target and self-delete, so a missed/fired task never lingers.
$tr.EndBoundary = $t.AddHours(2).ToString('s')
$s  = New-ScheduledTaskSettingsSet -WakeToRun -StartWhenAvailable -DeleteExpiredTaskAfter (New-TimeSpan -Minutes 5)
try {
    # Preferred: run as SYSTEM so it fires even if nobody is logged on (needs admin).
    $p = New-ScheduledTaskPrincipal -UserId 'NT AUTHORITY\SYSTEM' -RunLevel Highest
    Register-ScheduledTask -TaskName '__TASK__' -Action $a -Trigger $tr -Settings $s -Principal $p -Force | Out-Null
} catch {
    # Fallback (not elevated): run as the current user while logged on.
    $p = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType Interactive
    Register-ScheduledTask -TaskName '__TASK__' -Action $a -Trigger $tr -Settings $s -Principal $p -Force | Out-Null
}
Write-Output 'REGISTERED'
"#;

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

    #[cfg(windows)]
    {
        let at = target.format("%Y-%m-%dT%H:%M:%S").to_string();
        let script = REGISTER_PS.replace("__AT__", &at).replace("__TASK__", TASK_NAME);
        let res = run_cmd(
            "powershell",
            &["-NoProfile", "-NonInteractive", "-Command", &script],
            Duration::from_secs(30),
        )
        .await;
        if !res.stdout.contains("REGISTERED") {
            return Err(format!("could not register reboot task: {}", res.combined().trim()));
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

/// Returns the ISO datetime of a pending reboot, if it is still in the future
/// (or only just passed) and, on Windows, the task still exists.
pub async fn pending() -> Option<String> {
    let iso = std::fs::read_to_string(reboot_path()).ok()?;
    let iso = iso.trim().to_string();
    if iso.is_empty() {
        return None;
    }

    // Stale or unreadable timestamp -> clear it rather than show it forever.
    match DateTime::parse_from_rfc3339(&iso) {
        Ok(t) => {
            let age = Local::now().signed_duration_since(t.with_timezone(&Local));
            if age > ChronoDur::hours(STALE_AFTER_HOURS) {
                cancel().await;
                return None;
            }
        }
        Err(_) => {
            cancel().await;
            return None;
        }
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
