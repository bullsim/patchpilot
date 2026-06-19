//! In-app daily scheduler. Registers an OS task that runs PatchPilot headlessly
//! (`--silent --mode <mode>`) at a chosen time. Windows = Task Scheduler (elevated,
//! so no UAC at run time), macOS = launchd LaunchAgent, Linux = user systemd timer.

use crate::util::run_cmd;
use std::time::Duration;

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
const TASK_NAME: &str = "PatchPilot_DailyUpdates";

fn exe() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "patchpilot".into())
}

/// Apply (or remove if `enabled` is false) the daily schedule. `time` is "HH:mm".
pub async fn apply(enabled: bool, time: &str, mode: &str) -> Result<(), String> {
    if !enabled {
        remove().await;
        return Ok(());
    }
    // Validate HH:mm.
    let (h, m) = time.split_once(':').ok_or("time must be HH:mm")?;
    let _: u32 = h.trim().parse().map_err(|_| "bad hour")?;
    let _: u32 = m.trim().parse().map_err(|_| "bad minute")?;

    #[cfg(windows)]
    return apply_windows(time, mode).await;
    #[cfg(target_os = "macos")]
    return apply_macos(time, mode).await;
    #[cfg(target_os = "linux")]
    return apply_linux(time, mode).await;
    #[allow(unreachable_code)]
    Err("scheduling not supported on this OS".into())
}

pub async fn remove() {
    #[cfg(windows)]
    {
        run_cmd("schtasks", &["/Delete", "/TN", TASK_NAME, "/F"], Duration::from_secs(20)).await;
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(p) = mac_plist_path() {
            run_cmd("launchctl", &["unload", &p], Duration::from_secs(15)).await;
            let _ = std::fs::remove_file(&p);
        }
    }
    #[cfg(target_os = "linux")]
    {
        run_cmd("systemctl", &["--user", "disable", "--now", "patchpilot.timer"], Duration::from_secs(15)).await;
        if let Some(dir) = linux_unit_dir() {
            let _ = std::fs::remove_file(dir.join("patchpilot.timer"));
            let _ = std::fs::remove_file(dir.join("patchpilot.service"));
        }
    }
}

// ---------------- Windows ----------------
#[cfg(windows)]
async fn apply_windows(time: &str, mode: &str) -> Result<(), String> {
    let tr = format!("\"{}\" --silent --mode {}", exe(), mode);
    let res = run_cmd(
        "schtasks",
        &[
            "/Create", "/TN", TASK_NAME, "/TR", &tr, "/SC", "DAILY", "/ST", time,
            "/RL", "HIGHEST", "/F",
        ],
        Duration::from_secs(20),
    )
    .await;
    if res.code == Some(0) {
        Ok(())
    } else {
        Err(format!("schtasks failed: {}", res.combined().trim()))
    }
}

// ---------------- macOS ----------------
#[cfg(target_os = "macos")]
fn mac_plist_path() -> Option<String> {
    std::env::var("HOME")
        .ok()
        .map(|h| format!("{h}/Library/LaunchAgents/com.bullers.patchpilot.daily.plist"))
}

#[cfg(target_os = "macos")]
async fn apply_macos(time: &str, mode: &str) -> Result<(), String> {
    let (h, m) = time.split_once(':').unwrap();
    let path = mac_plist_path().ok_or("no HOME")?;
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>com.bullers.patchpilot.daily</string>
  <key>ProgramArguments</key>
  <array><string>{}</string><string>--silent</string><string>--mode</string><string>{}</string></array>
  <key>StartCalendarInterval</key>
  <dict><key>Hour</key><integer>{}</integer><key>Minute</key><integer>{}</integer></dict>
</dict></plist>"#,
        exe(), mode, h.trim(), m.trim()
    );
    std::fs::write(&path, plist).map_err(|e| e.to_string())?;
    run_cmd("launchctl", &["unload", &path], Duration::from_secs(15)).await;
    let res = run_cmd("launchctl", &["load", &path], Duration::from_secs(15)).await;
    if res.code == Some(0) { Ok(()) } else { Err("launchctl load failed".into()) }
}

// ---------------- Linux ----------------
#[cfg(target_os = "linux")]
fn linux_unit_dir() -> Option<std::path::PathBuf> {
    let base = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .or_else(|| std::env::var("HOME").ok().map(|h| format!("{h}/.config")))?;
    let dir = std::path::PathBuf::from(base).join("systemd/user");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

#[cfg(target_os = "linux")]
async fn apply_linux(time: &str, mode: &str) -> Result<(), String> {
    let dir = linux_unit_dir().ok_or("no config dir")?;
    let service = format!(
        "[Unit]\nDescription=PatchPilot daily update\n[Service]\nType=oneshot\nExecStart={} --silent --mode {}\n",
        exe(), mode
    );
    let timer = format!(
        "[Unit]\nDescription=PatchPilot daily timer\n[Timer]\nOnCalendar=*-*-* {}:00\nPersistent=true\n[Install]\nWantedBy=timers.target\n",
        time
    );
    std::fs::write(dir.join("patchpilot.service"), service).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("patchpilot.timer"), timer).map_err(|e| e.to_string())?;
    run_cmd("systemctl", &["--user", "daemon-reload"], Duration::from_secs(15)).await;
    let res = run_cmd("systemctl", &["--user", "enable", "--now", "patchpilot.timer"], Duration::from_secs(15)).await;
    if res.code == Some(0) { Ok(()) } else { Err("systemctl enable failed".into()) }
}
