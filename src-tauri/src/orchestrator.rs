use crate::config::AppConfig;
use crate::model::{Category, ComponentStatus, RunMode, RunSummary, Status};
use crate::registry::selection;
use crate::system_info::SystemInfo;
use crate::updaters;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Sink for status + log + summary. GUI emits Tauri events; CLI prints/logs.
pub trait Reporter: Send + Sync {
    fn emit(&self, s: &ComponentStatus);
    fn log(&self, line: &str);
    fn finished(&self, summary: &RunSummary);
}

/// Wraps the reporter, records latest status per component, and a reboot flag.
pub struct Tracker {
    inner: Arc<dyn Reporter>,
    latest: Mutex<HashMap<String, Status>>,
    reboot: AtomicBool,
}

impl Tracker {
    pub fn new(inner: Arc<dyn Reporter>) -> Arc<Self> {
        Arc::new(Tracker {
            inner,
            latest: Mutex::new(HashMap::new()),
            reboot: AtomicBool::new(false),
        })
    }
    fn record(&self, s: &ComponentStatus) {
        self.latest.lock().unwrap().insert(s.id.clone(), s.status);
        self.inner.emit(s);
    }
    pub fn log(&self, line: &str) {
        self.inner.log(line);
    }
}

/// Per-component reporting handle passed to each updater.
pub struct Reporter2 {
    pub tracker: Arc<Tracker>,
    pub id: String,
    pub name: String,
    pub category: Category,
}

impl Reporter2 {
    pub fn set(&self, status: Status, detail: &str, progress: i32) {
        let s = ComponentStatus {
            id: self.id.clone(),
            name: self.name.clone(),
            category: self.category,
            status,
            detail: detail.to_string(),
            progress,
        };
        self.tracker.log(&format!("[{}] {:?} - {}", self.name, status, detail));
        self.tracker.record(&s);
    }
    pub fn request_reboot(&self) {
        self.tracker.reboot.store(true, Ordering::SeqCst);
    }
}

/// Context handed to each updater.
pub struct Ctx {
    pub rep: Reporter2,
    // Kept for mac/linux updaters that will branch on hardware.
    #[allow(dead_code)]
    pub sys: Arc<SystemInfo>,
    pub cancel: Arc<AtomicBool>,
    pub ha_url: String,
    pub ha_token: String,
}

impl Ctx {
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
}

/// Build the initial "Pending" card list for a mode (used by the UI before a run).
pub fn plan(mode: RunMode, sys: &SystemInfo, cfg: &AppConfig) -> Vec<ComponentStatus> {
    selection(mode, sys, cfg)
        .into_iter()
        .map(|m| ComponentStatus {
            id: m.id.to_string(),
            name: m.name.to_string(),
            category: m.category,
            status: Status::Pending,
            detail: "Queued".into(),
            progress: -1,
        })
        .collect()
}

/// Run all selected components sequentially, reporting as we go.
pub async fn run_all(
    mode: RunMode,
    sys: Arc<SystemInfo>,
    cfg: AppConfig,
    reporter: Arc<dyn Reporter>,
    cancel: Arc<AtomicBool>,
) -> RunSummary {
    let comps = selection(mode, &sys, &cfg);
    run_list(mode, comps, sys, cfg, reporter, cancel).await
}

/// Run a single component by id (used when a card is clicked in the UI).
pub async fn run_one(
    id: &str,
    sys: Arc<SystemInfo>,
    cfg: AppConfig,
    reporter: Arc<dyn Reporter>,
    cancel: Arc<AtomicBool>,
) -> RunSummary {
    let comps: Vec<crate::registry::ComponentMeta> =
        crate::registry::find(id).into_iter().collect();
    run_list(RunMode::All, comps, sys, cfg, reporter, cancel).await
}

async fn run_list(
    mode: RunMode,
    comps: Vec<crate::registry::ComponentMeta>,
    sys: Arc<SystemInfo>,
    cfg: AppConfig,
    reporter: Arc<dyn Reporter>,
    cancel: Arc<AtomicBool>,
) -> RunSummary {
    let started = Instant::now();
    let tracker = Tracker::new(reporter);

    tracker.log(&format!(
        "=== PatchPilot run [{mode:?}] on {} {} — {} components ===",
        sys.manufacturer,
        sys.model,
        comps.len()
    ));

    for meta in &comps {
        if cancel.load(Ordering::SeqCst) {
            tracker.log("Run cancelled by user.");
            break;
        }
        let rep = Reporter2 {
            tracker: tracker.clone(),
            id: meta.id.to_string(),
            name: meta.name.to_string(),
            category: meta.category,
        };
        rep.set(Status::Running, "Starting…", 0);
        let ctx = Ctx {
            rep,
            sys: sys.clone(),
            cancel: cancel.clone(),
            ha_url: cfg.ha_url.clone(),
            ha_token: cfg.ha_token.clone(),
        };
        updaters::run(meta.id, &ctx).await;
    }

    // Tally final statuses.
    let latest = tracker.latest.lock().unwrap();
    let mut ok = 0;
    let mut warn = 0;
    let mut fail = 0;
    let mut skip = 0;
    for st in latest.values() {
        match st {
            Status::Success => ok += 1,
            Status::Warning => warn += 1,
            Status::Failed => fail += 1,
            Status::Skipped => skip += 1,
            _ => {}
        }
    }
    drop(latest);

    let summary = RunSummary {
        mode,
        ok,
        warn,
        fail,
        skip,
        duration_secs: started.elapsed().as_secs(),
        reboot_required: tracker.reboot.load(Ordering::SeqCst),
    };
    tracker.log(&format!(
        "=== Done: ✓{ok} ⚠{warn} ✗{fail} ○{skip} in {}s ===",
        summary.duration_secs
    ));
    tracker.inner.finished(&summary);
    summary
}
