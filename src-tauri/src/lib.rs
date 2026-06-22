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

#[tauri::command]
async fn plan_run(mode: RunMode, state: State<'_, AppState>) -> Result<Vec<ComponentStatus>, String> {
    let sys = cached_sys(&state).await;
    let cfg = config::load();
    Ok(orchestrator::plan(mode, &sys, &cfg))
}

#[tauri::command]
async fn start_run(mode: RunMode, app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if state.running.swap(true, Ordering::SeqCst) {
        return Err("A run is already in progress".into());
    }
    state.cancel.store(false, Ordering::SeqCst);

    let sys = cached_sys(&state).await;
    let cfg = config::load();
    let cancel = state.cancel.clone();
    let running = state.running.clone();
    let report_url = cfg.report_url.clone();
    let reporter: Arc<dyn Reporter> = Arc::new(EventReporter {
        app: app.clone(),
        log: FileLog::new(),
    });

    tauri::async_runtime::spawn(async move {
        let summary = orchestrator::run_all(mode, sys.clone(), cfg, reporter, cancel).await;
        record_history(&summary);
        report_status(&report_url, &sys, &summary).await;
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
    let report_url = cfg.report_url.clone();
    let reporter: Arc<dyn Reporter> = Arc::new(EventReporter {
        app: app.clone(),
        log: FileLog::new(),
    });

    tauri::async_runtime::spawn(async move {
        let summary = orchestrator::run_one(&id, sys.clone(), cfg, reporter, cancel).await;
        record_history(&summary);
        report_status(&report_url, &sys, &summary).await;
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

/// POST a small status report to the fleet dashboard (best effort, via curl).
async fn report_status(report_url: &str, sys: &SystemInfo, summary: &model::RunSummary) {
    if report_url.trim().is_empty() {
        return;
    }
    let payload = serde_json::json!({
        "hostname": hostname(),
        "os": sys.os,
        "manufacturer": sys.manufacturer,
        "model": sys.model,
        "version": env!("CARGO_PKG_VERSION"),
        "mode": summary.mode,
        "ok": summary.ok,
        "warn": summary.warn,
        "fail": summary.fail,
        "skip": summary.skip,
        "rebootRequired": summary.reboot_required,
        "durationSecs": summary.duration_secs,
        "timestamp": chrono::Local::now().to_rfc3339(),
    });
    let tmp = paths::app_dir().join("last_report.json");
    if std::fs::write(&tmp, payload.to_string()).is_err() {
        return;
    }
    let data = format!("@{}", tmp.display());
    let _ = util::run_cmd(
        "curl",
        &[
            "-s", "-m", "15", "-X", "POST", "-H", "Content-Type: application/json",
            "--data-binary", &data, report_url,
        ],
        std::time::Duration::from_secs(20),
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
        .manage(AppState {
            cancel: Arc::new(AtomicBool::new(false)),
            running: Arc::new(AtomicBool::new(false)),
            sys: Arc::new(Mutex::new(None)),
        })
        .setup(|app| {
            // System tray: run/show/quit + click-to-show.
            let run_all = MenuItem::with_id(app, "run_all", "Run all updates", true, None::<&str>)?;
            let show = MenuItem::with_id(app, "show", "Show PatchPilot", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&run_all, &show, &quit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("PatchPilot")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "run_all" => {
                        show_main(app);
                        let _ = app.emit("tray-run-all", ());
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
        let report_url = cfg.report_url.clone();
        let auto_reboot = cfg.auto_reboot;
        let reporter: Arc<dyn Reporter> = Arc::new(CliReporter { log: FileLog::new() });
        let cancel = Arc::new(AtomicBool::new(false));
        let summary = orchestrator::run_all(mode, sys.clone(), cfg, reporter, cancel).await;
        record_history(&summary);
        report_status(&report_url, &sys, &summary).await;
        // Unattended firmware updates that need a reboot: restart so they finish.
        if summary.reboot_required && auto_reboot {
            let _ = reboot::schedule("now").await;
        }
    });
}
