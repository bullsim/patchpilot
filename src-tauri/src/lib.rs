mod config;
mod model;
mod orchestrator;
mod paths;
mod registry;
mod reboot;
mod schedule;
mod system_info;
mod updaters;
mod util;

use config::AppConfig;
use model::{ComponentStatus, RunMode, RunSummary};
use orchestrator::Reporter;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use system_info::SystemInfo;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_notification::NotificationExt;

// ---------------- shared run state ----------------

struct AppState {
    cancel: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    sys: Arc<Mutex<Option<Arc<SystemInfo>>>>,
}

async fn cached_sys(state: &State<'_, AppState>) -> Arc<SystemInfo> {
    if let Some(s) = state.sys.lock().unwrap().clone() {
        return s;
    }
    // Instant first paint from the last cached detection; refresh in the background
    // so the UI never freezes on a slow (esp. elevated) winget probe.
    if let Some(cached) = system_info::load_cached() {
        let arc = Arc::new(cached);
        *state.sys.lock().unwrap() = Some(arc.clone());
        let slot = state.sys.clone();
        tauri::async_runtime::spawn(async move {
            let fresh = Arc::new(system_info::detect().await);
            *slot.lock().unwrap() = Some(fresh);
        });
        return arc;
    }
    let s = Arc::new(system_info::detect().await);
    *state.sys.lock().unwrap() = Some(s.clone());
    s
}

// ---------------- logging ----------------

struct FileLog(Mutex<std::fs::File>);

/// Delete run logs older than 7 days.
fn rotate_logs() {
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(7 * 24 * 3600);
    if let Ok(rd) = std::fs::read_dir(paths::logs_dir()) {
        for e in rd.flatten() {
            if e.path().extension().map(|x| x == "log").unwrap_or(false) {
                if let Ok(modified) = e.metadata().and_then(|m| m.modified()) {
                    if modified < cutoff {
                        let _ = std::fs::remove_file(e.path());
                    }
                }
            }
        }
    }
}

impl FileLog {
    fn new() -> Arc<Self> {
        rotate_logs();
        let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let path = paths::logs_dir().join(format!("run_{ts}.log"));
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap_or_else(|_| std::fs::File::create(paths::logs_dir().join("run.log")).unwrap());
        Arc::new(FileLog(Mutex::new(file)))
    }
    fn line(&self, s: &str) {
        if let Ok(mut f) = self.0.lock() {
            let ts = chrono::Local::now().format("%H:%M:%S");
            let _ = writeln!(f, "[{ts}] {s}");
        }
    }
}

// ---------------- reporters ----------------

/// GUI reporter: emits Tauri events + writes the log file.
struct EventReporter {
    app: AppHandle,
    log: Arc<FileLog>,
}

impl Reporter for EventReporter {
    fn emit(&self, s: &ComponentStatus) {
        let _ = self.app.emit("component-status", s.clone());
    }
    fn log(&self, line: &str) {
        self.log.line(line);
        let _ = self.app.emit("log-line", line.to_string());
    }
    fn finished(&self, summary: &RunSummary) {
        let _ = self.app.emit("run-finished", summary.clone());
    }
}

/// CLI reporter for --silent scheduled runs.
struct CliReporter {
    log: Arc<FileLog>,
}

impl Reporter for CliReporter {
    fn emit(&self, _s: &ComponentStatus) {}
    fn log(&self, line: &str) {
        self.log.line(line);
        println!("{line}");
    }
    fn finished(&self, summary: &RunSummary) {
        self.log.line(&format!("FINISHED: {summary:?}"));
    }
}

// ---------------- commands ----------------

#[tauri::command]
async fn get_system_info(state: State<'_, AppState>) -> Result<SystemInfo, String> {
    Ok((*cached_sys(&state).await).clone())
}

#[tauri::command]
fn get_config() -> Result<AppConfig, String> {
    Ok(config::load())
}

#[tauri::command]
async fn save_config(config: AppConfig) -> Result<(), String> {
    let old = config::load();
    config::save(&config).map_err(|e| e.to_string())?;

    // Unpin winget packages that were removed from the exclude list.
    #[cfg(windows)]
    for id in &old.winget_excludes {
        let id = id.trim();
        if !id.is_empty() && !config.winget_excludes.iter().any(|n| n.trim() == id) {
            let _ = util::run_cmd(
                "winget",
                &["pin", "remove", "--id", id, "--exact"],
                std::time::Duration::from_secs(30),
            )
            .await;
        }
    }

    // Keep the OS scheduled task in sync with the chosen schedule.
    schedule::apply(
        config.schedule_enabled,
        &config.schedule_time,
        config.scheduled_run_mode.as_str(),
    )
    .await
}

fn portable_config_path() -> std::path::PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(home).join("patchpilot-config.json")
}

/// Export the current config to ~/patchpilot-config.json for copying to other machines.
#[tauri::command]
fn export_config() -> Result<String, String> {
    let cfg = config::load();
    let path = portable_config_path();
    let txt = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
    std::fs::write(&path, txt).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

/// Import config from ~/patchpilot-config.json (applies + re-arms the schedule).
#[tauri::command]
async fn import_config() -> Result<AppConfig, String> {
    let path = portable_config_path();
    let txt = std::fs::read_to_string(&path)
        .map_err(|_| format!("No config found at {}", path.display()))?;
    let cfg: AppConfig = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
    config::save(&cfg).map_err(|e| e.to_string())?;
    schedule::apply(cfg.schedule_enabled, &cfg.schedule_time, cfg.scheduled_run_mode.as_str()).await?;
    Ok(cfg)
}

/// Read all machines' status from the shared fleet Gist.
#[tauri::command]
async fn get_fleet() -> Result<Vec<serde_json::Value>, String> {
    let cfg = config::load();
    if cfg.fleet_gist.trim().is_empty() || cfg.fleet_token.trim().is_empty() {
        return Err("Fleet not configured (set Gist id + token in Settings)".into());
    }
    let auth = format!("Authorization: Bearer {}", cfg.fleet_token.trim());
    let url = format!("https://api.github.com/gists/{}", cfg.fleet_gist.trim());
    let res = util::run_cmd(
        "curl",
        &[
            "-s", "-m", "20", "-H", &auth, "-H", "Accept: application/vnd.github+json",
            "-H", "User-Agent: PatchPilot", &url,
        ],
        std::time::Duration::from_secs(25),
    )
    .await;
    let v: serde_json::Value =
        serde_json::from_str(&res.stdout).map_err(|_| "Couldn't reach the fleet Gist".to_string())?;
    let files = v
        .get("files")
        .and_then(|f| f.as_object())
        .ok_or("Gist has no files (check id/token)")?;
    let mut out = Vec::new();
    for f in files.values() {
        if let Some(content) = f.get("content").and_then(|c| c.as_str()) {
            if let Ok(m) = serde_json::from_str::<serde_json::Value>(content) {
                out.push(m);
            }
        }
    }
    Ok(out)
}

// ---------------- remote commands (via the fleet Gist) ----------------

async fn gist_get(gist: &str, token: &str) -> Option<serde_json::Value> {
    let auth = format!("Authorization: Bearer {}", token.trim());
    let url = format!("https://api.github.com/gists/{}", gist.trim());
    let res = util::run_cmd(
        "curl",
        &["-s", "-m", "20", "-H", &auth, "-H", "Accept: application/vnd.github+json", "-H", "User-Agent: PatchPilot", &url],
        std::time::Duration::from_secs(25),
    )
    .await;
    serde_json::from_str(&res.stdout).ok()
}

async fn gist_put_file(gist: &str, token: &str, filename: &str, content: &str) {
    let mut files = serde_json::Map::new();
    files.insert(filename.to_string(), serde_json::json!({ "content": content }));
    let body = serde_json::json!({ "files": files }).to_string();
    let tmp = paths::app_dir().join("gist_put.json");
    if std::fs::write(&tmp, body).is_err() {
        return;
    }
    let auth = format!("Authorization: Bearer {}", token.trim());
    let url = format!("https://api.github.com/gists/{}", gist.trim());
    let data = format!("@{}", tmp.display());
    let _ = util::run_cmd(
        "curl",
        &[
            "-s", "-m", "20", "-X", "PATCH", "-H", &auth, "-H", "Accept: application/vnd.github+json",
            "-H", "User-Agent: PatchPilot", "-H", "Content-Type: application/json", "--data-binary", &data, &url,
        ],
        std::time::Duration::from_secs(25),
    )
    .await;
}

fn read_commands(gist: &serde_json::Value) -> serde_json::Value {
    gist.get("files")
        .and_then(|f| f.get("_commands.json"))
        .and_then(|f| f.get("content"))
        .and_then(|c| c.as_str())
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

/// Queue a command (run-all / run-software / run-firmware / reboot) for a machine.
#[tauri::command]
async fn send_command(host: String, action: String) -> Result<(), String> {
    let cfg = config::load();
    if cfg.fleet_gist.trim().is_empty() || cfg.fleet_token.trim().is_empty() {
        return Err("Fleet not configured".into());
    }
    let gist = gist_get(&cfg.fleet_gist, &cfg.fleet_token).await.ok_or("Couldn't reach the fleet Gist")?;
    let mut cmds = read_commands(&gist);
    let ts = chrono::Local::now().timestamp_millis();
    if let Some(obj) = cmds.as_object_mut() {
        obj.insert(host, serde_json::json!({ "action": action, "ts": ts }));
    }
    gist_put_file(&cfg.fleet_gist, &cfg.fleet_token, "_commands.json", &cmds.to_string()).await;
    Ok(())
}

/// Poll the Gist for a command targeting this machine and run it once.
async fn check_commands(app: &AppHandle) {
    let cfg = config::load();
    if cfg.fleet_gist.trim().is_empty() || cfg.fleet_token.trim().is_empty() {
        return;
    }
    let Some(gist) = gist_get(&cfg.fleet_gist, &cfg.fleet_token).await else {
        return;
    };
    let cmds = read_commands(&gist);
    let host = hostname();
    let Some(entry) = cmds.get(&host) else {
        return;
    };
    let ts = entry.get("ts").and_then(|t| t.as_i64()).unwrap_or(0);
    let action = entry.get("action").and_then(|a| a.as_str()).unwrap_or("").to_string();
    let last_path = paths::app_dir().join("last_cmd.txt");
    let last: i64 = std::fs::read_to_string(&last_path).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
    if ts <= last {
        return;
    }
    let _ = std::fs::write(&last_path, ts.to_string());
    match action.as_str() {
        "run-all" => { let _ = begin_run(app.clone(), RunMode::All).await; }
        "run-software" => { let _ = begin_run(app.clone(), RunMode::Software).await; }
        "run-firmware" => { let _ = begin_run(app.clone(), RunMode::Firmware).await; }
        "reboot" => { let _ = reboot::schedule("now").await; }
        _ => {}
    }
}

// ---------------- central policy (via the fleet Gist) ----------------

fn read_policy(gist: &serde_json::Value) -> Option<serde_json::Value> {
    gist.get("files")
        .and_then(|f| f.get("_policy.json"))
        .and_then(|f| f.get("content"))
        .and_then(|c| c.as_str())
        .and_then(|s| serde_json::from_str(s).ok())
}

/// Publish this machine's settings as the shared fleet policy. NO secrets are
/// included (tokens, webhooks, HA credentials stay local).
#[tauri::command]
async fn publish_policy() -> Result<(), String> {
    let cfg = config::load();
    if cfg.fleet_gist.trim().is_empty() || cfg.fleet_token.trim().is_empty() {
        return Err("Fleet not configured (set Gist id + token in Settings)".into());
    }
    let policy = serde_json::json!({
        "components": cfg.components,
        "wingetExcludes": cfg.winget_excludes,
        "complianceDays": cfg.compliance_days,
        "scheduleEnabled": cfg.schedule_enabled,
        "scheduleTime": cfg.schedule_time,
        "scheduledRunMode": cfg.scheduled_run_mode.as_str(),
        "autoReboot": cfg.auto_reboot,
        "updatedBy": hostname(),
        "updatedAt": chrono::Local::now().to_rfc3339(),
    })
    .to_string();
    gist_put_file(&cfg.fleet_gist, &cfg.fleet_token, "_policy.json", &policy).await;
    Ok(())
}

/// Read the current shared fleet policy (`_policy.json`), or null if none exists.
#[tauri::command]
async fn get_policy() -> Result<Option<serde_json::Value>, String> {
    let cfg = config::load();
    if cfg.fleet_gist.trim().is_empty() || cfg.fleet_token.trim().is_empty() {
        return Err("Fleet not configured (set Gist id + token in Settings)".into());
    }
    let gist = gist_get(&cfg.fleet_gist, &cfg.fleet_token)
        .await
        .ok_or("Couldn't reach the fleet Gist")?;
    Ok(read_policy(&gist))
}

/// Pull and apply the fleet policy immediately (instead of waiting for the poller).
#[tauri::command]
async fn apply_policy_now(app: AppHandle) -> Result<(), String> {
    let cfg = config::load();
    if !cfg.follow_policy {
        return Err("Turn on \"Follow fleet policy\" first".into());
    }
    if cfg.fleet_gist.trim().is_empty() || cfg.fleet_token.trim().is_empty() {
        return Err("Fleet not configured (set Gist id + token in Settings)".into());
    }
    check_policy(&app).await;
    Ok(())
}

/// If this machine follows the fleet policy, pull `_policy.json` and apply the
/// shared settings (component toggles, schedule, excludes, etc.). Opt-in; secrets
/// are never touched. Re-arms the OS schedule only when schedule fields change.
async fn check_policy(app: &AppHandle) {
    let cfg = config::load();
    if !cfg.follow_policy || cfg.fleet_gist.trim().is_empty() || cfg.fleet_token.trim().is_empty() {
        return;
    }
    let Some(gist) = gist_get(&cfg.fleet_gist, &cfg.fleet_token).await else {
        return;
    };
    let Some(policy) = read_policy(&gist) else {
        return;
    };

    let mut cfg = cfg;
    let mut changed = false;
    let mut schedule_changed = false;

    if let Some(map) = policy.get("components").and_then(|v| v.as_object()) {
        for (k, v) in map {
            if let Some(b) = v.as_bool() {
                if cfg.components.get(k) != Some(&b) {
                    cfg.components.insert(k.clone(), b);
                    changed = true;
                }
            }
        }
    }
    if let Some(arr) = policy.get("wingetExcludes").and_then(|v| v.as_array()) {
        let ex: Vec<String> = arr.iter().filter_map(|s| s.as_str().map(str::to_string)).collect();
        if cfg.winget_excludes != ex {
            cfg.winget_excludes = ex;
            changed = true;
        }
    }
    if let Some(n) = policy.get("complianceDays").and_then(|v| v.as_u64()) {
        if cfg.compliance_days != n as u32 {
            cfg.compliance_days = n as u32;
            changed = true;
        }
    }
    if let Some(b) = policy.get("scheduleEnabled").and_then(|v| v.as_bool()) {
        if cfg.schedule_enabled != b {
            cfg.schedule_enabled = b;
            changed = true;
            schedule_changed = true;
        }
    }
    if let Some(s) = policy.get("scheduleTime").and_then(|v| v.as_str()) {
        if cfg.schedule_time != s {
            cfg.schedule_time = s.to_string();
            changed = true;
            schedule_changed = true;
        }
    }
    if let Some(s) = policy.get("scheduledRunMode").and_then(|v| v.as_str()) {
        let m = RunMode::from_str(s);
        if cfg.scheduled_run_mode.as_str() != m.as_str() {
            cfg.scheduled_run_mode = m;
            changed = true;
            schedule_changed = true;
        }
    }
    if let Some(b) = policy.get("autoReboot").and_then(|v| v.as_bool()) {
        if cfg.auto_reboot != b {
            cfg.auto_reboot = b;
            changed = true;
        }
    }

    if changed {
        let _ = config::save(&cfg);
        if schedule_changed {
            let _ = schedule::apply(
                cfg.schedule_enabled,
                &cfg.schedule_time,
                cfg.scheduled_run_mode.as_str(),
            )
            .await;
        }
        // Tell an open UI to reload, and let the user know the policy landed.
        let _ = app.emit("config-changed", ());
        let _ = app.notification().builder()
            .title("PatchPilot — fleet policy applied")
            .body("This machine's settings were updated from the shared fleet policy.")
            .show();
    }
}

#[tauri::command]
async fn plan_run(mode: RunMode, state: State<'_, AppState>) -> Result<Vec<ComponentStatus>, String> {
    let sys = cached_sys(&state).await;
    let cfg = config::load();
    Ok(orchestrator::plan(mode, &sys, &cfg))
}

#[tauri::command]
async fn start_run(mode: RunMode, app: AppHandle) -> Result<(), String> {
    begin_run(app, mode).await
}

/// Start a full run in the given mode. Usable from a command or the command poller.
async fn begin_run(app: AppHandle, mode: RunMode) -> Result<(), String> {
    let state = app.state::<AppState>();
    if state.running.swap(true, Ordering::SeqCst) {
        return Err("A run is already in progress".into());
    }
    state.cancel.store(false, Ordering::SeqCst);

    let sys = cached_sys(&state).await;
    let cfg = config::load();
    let cancel = state.cancel.clone();
    let running = state.running.clone();
    let fleet_gist = cfg.fleet_gist.clone();
    let fleet_token = cfg.fleet_token.clone();
    let reporter: Arc<dyn Reporter> = Arc::new(EventReporter {
        app: app.clone(),
        log: FileLog::new(),
    });

    tauri::async_runtime::spawn(async move {
        let summary = orchestrator::run_all(mode, sys.clone(), cfg, reporter, cancel).await;
        record_history(&summary);
        report_status(&fleet_gist, &fleet_token, &sys, &summary).await;
        running.store(false, Ordering::SeqCst);
    });
    Ok(())
}

#[tauri::command]
async fn run_one(id: String, app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if state.running.swap(true, Ordering::SeqCst) {
        return Err("A run is already in progress".into());
    }
    state.cancel.store(false, Ordering::SeqCst);

    let sys = cached_sys(&state).await;
    let cfg = config::load();
    let cancel = state.cancel.clone();
    let running = state.running.clone();
    let fleet_gist = cfg.fleet_gist.clone();
    let fleet_token = cfg.fleet_token.clone();
    let reporter: Arc<dyn Reporter> = Arc::new(EventReporter {
        app: app.clone(),
        log: FileLog::new(),
    });

    tauri::async_runtime::spawn(async move {
        let summary = orchestrator::run_one(&id, sys.clone(), cfg, reporter, cancel).await;
        record_history(&summary);
        report_status(&fleet_gist, &fleet_token, &sys, &summary).await;
        running.store(false, Ordering::SeqCst);
    });
    Ok(())
}

#[tauri::command]
fn cancel_run(state: State<'_, AppState>) -> Result<(), String> {
    state.cancel.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
fn open_latest_log() -> Result<(), String> {
    let dir = paths::logs_dir();
    let latest = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "log").unwrap_or(false))
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
    let path = match latest {
        Some(e) => e.path(),
        None => return Err("No logs yet".into()),
    };
    open_path(&path);
    Ok(())
}

fn open_path(path: &std::path::Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", &path.to_string_lossy()])
            .spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}

#[tauri::command]
async fn schedule_reboot(when: String) -> Result<String, String> {
    reboot::schedule(&when).await
}

#[tauri::command]
async fn cancel_reboot() -> Result<(), String> {
    reboot::cancel().await;
    Ok(())
}

#[tauri::command]
async fn get_pending_reboot() -> Result<Option<String>, String> {
    Ok(reboot::pending().await)
}

// ---------------- fleet reporting ----------------

fn hostname() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

/// Write this machine's status into the shared fleet Gist (best effort, via curl).
async fn report_status(gist: &str, token: &str, sys: &SystemInfo, summary: &model::RunSummary) {
    if gist.trim().is_empty() || token.trim().is_empty() {
        return;
    }
    let host = hostname();
    let follow_policy = config::load().follow_policy;
    let status = serde_json::json!({
        "hostname": host,
        "os": sys.os,
        "manufacturer": sys.manufacturer,
        "model": sys.model,
        "version": env!("CARGO_PKG_VERSION"),
        "followPolicy": follow_policy,
        "mode": summary.mode,
        "ok": summary.ok,
        "warn": summary.warn,
        "fail": summary.fail,
        "skip": summary.skip,
        "rebootRequired": summary.reboot_required,
        "durationSecs": summary.duration_secs,
        "components": summary.components,
        "timestamp": chrono::Local::now().to_rfc3339(),
    })
    .to_string();
    let mut files = serde_json::Map::new();
    let fname = format!("{}.json", host.replace(['/', '\\', ' '], "_"));
    files.insert(fname, serde_json::json!({ "content": status }));
    let body = serde_json::json!({ "files": files }).to_string();

    let tmp = paths::app_dir().join("fleet_body.json");
    if std::fs::write(&tmp, body).is_err() {
        return;
    }
    let auth = format!("Authorization: Bearer {}", token.trim());
    let url = format!("https://api.github.com/gists/{}", gist.trim());
    let data = format!("@{}", tmp.display());
    let _ = util::run_cmd(
        "curl",
        &[
            "-s", "-m", "20", "-X", "PATCH", "-H", &auth,
            "-H", "Accept: application/vnd.github+json", "-H", "User-Agent: PatchPilot",
            "-H", "Content-Type: application/json", "--data-binary", &data, &url,
        ],
        std::time::Duration::from_secs(25),
    )
    .await;
}

/// Show + focus the main window (from tray).
fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

// ---------------- run history ----------------

fn record_history(summary: &model::RunSummary) {
    let path = paths::app_dir().join("history.json");
    let mut list: Vec<serde_json::Value> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    list.insert(
        0,
        serde_json::json!({
            "timestamp": chrono::Local::now().to_rfc3339(),
            "hostname": hostname(),
            "mode": summary.mode,
            "ok": summary.ok,
            "warn": summary.warn,
            "fail": summary.fail,
            "skip": summary.skip,
            "durationSecs": summary.duration_secs,
            "rebootRequired": summary.reboot_required,
        }),
    );
    list.truncate(50);
    if let Ok(txt) = serde_json::to_string_pretty(&list) {
        let _ = std::fs::write(&path, txt);
    }
}

#[tauri::command]
fn get_history() -> Vec<serde_json::Value> {
    std::fs::read_to_string(paths::app_dir().join("history.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

// ---------------- elevation (Windows) ----------------

/// Reliable admin check via the Windows token (IsInRole).
#[cfg(windows)]
pub fn is_elevated() -> bool {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "if(([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){exit 0}else{exit 1}",
        ])
        .creation_flags(0x0800_0000)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Relaunch this exe elevated via UAC. Returns true if the elevated copy launched.
#[cfg(windows)]
pub fn elevate() -> bool {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let path = exe.display().to_string().replace('\'', "''");
    let ps = format!(
        "try {{ Start-Process -FilePath '{path}' -ArgumentList '--no-elevate' -Verb RunAs; exit 0 }} catch {{ exit 1 }}"
    );
    Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps])
        .creation_flags(0x0800_0000)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[tauri::command]
fn is_admin() -> bool {
    #[cfg(windows)]
    {
        is_elevated()
    }
    #[cfg(not(windows))]
    {
        true // mac/linux updaters prompt per-command (osascript/pkexec)
    }
}

#[tauri::command]
fn relaunch_elevated() -> Result<(), String> {
    #[cfg(windows)]
    {
        if elevate() {
            std::process::exit(0);
        }
        return Err("Elevation was cancelled".into());
    }
    #[cfg(not(windows))]
    Ok(())
}

// ---------------- entry points ----------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(AppState {
            cancel: Arc::new(AtomicBool::new(false)),
            running: Arc::new(AtomicBool::new(false)),
            sys: Arc::new(Mutex::new(None)),
        })
        .setup(|app| {
            // System tray: run/show/quit + click-to-show.
            let run_all = MenuItem::with_id(app, "run_all", "Run all updates", true, None::<&str>)?;
            let run_sw = MenuItem::with_id(app, "run_software", "Run software only", true, None::<&str>)?;
            let run_fw = MenuItem::with_id(app, "run_firmware", "Run firmware only", true, None::<&str>)?;
            let show = MenuItem::with_id(app, "show", "Show PatchPilot", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&run_all, &run_sw, &run_fw, &show, &quit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("PatchPilot")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "run_all" => {
                        show_main(app);
                        let _ = app.emit("tray-run", "All");
                    }
                    "run_software" => {
                        show_main(app);
                        let _ = app.emit("tray-run", "Software");
                    }
                    "run_firmware" => {
                        show_main(app);
                        let _ = app.emit("tray-run", "Firmware");
                    }
                    "show" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;

            // Poll the fleet Gist for remote commands targeting this machine.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                loop {
                    check_commands(&handle).await;
                    check_policy(&handle).await;
                    tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Close to tray instead of quitting; use the tray's Quit to exit.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_system_info,
            get_config,
            save_config,
            plan_run,
            start_run,
            run_one,
            cancel_run,
            open_latest_log,
            schedule_reboot,
            cancel_reboot,
            get_pending_reboot,
            is_admin,
            relaunch_elevated,
            get_history,
            export_config,
            import_config,
            get_fleet,
            send_command,
            publish_policy,
            get_policy,
            apply_policy_now,
        ])
        .run(tauri::generate_context!())
        .expect("error while running PatchPilot");
}

/// Headless run for scheduled tasks: `patchpilot --silent --mode All`.
pub fn run_silent(args: &[String]) {
    let mut mode = config::load().scheduled_run_mode;
    if let Some(pos) = args.iter().position(|a| a == "--mode") {
        if let Some(m) = args.get(pos + 1) {
            mode = RunMode::from_str(m);
        }
    }
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async {
        let sys = Arc::new(system_info::detect().await);
        let cfg = config::load();
        let fleet_gist = cfg.fleet_gist.clone();
    let fleet_token = cfg.fleet_token.clone();
        let auto_reboot = cfg.auto_reboot;
        let reporter: Arc<dyn Reporter> = Arc::new(CliReporter { log: FileLog::new() });
        let cancel = Arc::new(AtomicBool::new(false));
        let summary = orchestrator::run_all(mode, sys.clone(), cfg, reporter, cancel).await;
        record_history(&summary);
        report_status(&fleet_gist, &fleet_token, &sys, &summary).await;
        // Unattended firmware updates that need a reboot: restart so they finish.
        if summary.reboot_required && auto_reboot {
            let _ = reboot::schedule("now").await;
        }
    });
}
