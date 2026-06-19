use crate::model::RunMode;
use crate::paths::config_path;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Component display names for the current OS (must match registry.rs).
#[cfg(windows)]
pub const COMPONENT_NAMES: &[&str] = &[
    "Windows Update",
    "Microsoft Store",
    "Winget Packages",
    "Microsoft Office",
    "Dell Stack",
    "Surface Stack",
    "Nvidia Stack",
    "Intel GPU Stack",
    "Razer Stack",
    "Logitech Stack",
    "Crucial Stack",
    "Home Assistant",
];

#[cfg(target_os = "macos")]
pub const COMPONENT_NAMES: &[&str] =
    &["macOS Software Update", "Homebrew", "Mac App Store", "Home Assistant"];

#[cfg(target_os = "linux")]
pub const COMPONENT_NAMES: &[&str] =
    &["APT Packages", "Flatpak", "Snap", "Firmware (fwupd)", "Home Assistant"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default = "default_mode")]
    pub scheduled_run_mode: RunMode,
    #[serde(default)]
    pub teams_webhook: String,
    /// Fleet dashboard endpoint; empty disables reporting.
    #[serde(default = "default_report_url")]
    pub report_url: String,
    /// Daily auto-run schedule.
    #[serde(default)]
    pub schedule_enabled: bool,
    #[serde(default = "default_schedule_time")]
    pub schedule_time: String,
    /// Home Assistant base URL (e.g. http://homeassistant.local:8123); empty disables it.
    #[serde(default)]
    pub ha_url: String,
    /// Home Assistant long-lived access token.
    #[serde(default)]
    pub ha_token: String,
    #[serde(default)]
    pub components: BTreeMap<String, bool>,
}

fn default_mode() -> RunMode {
    RunMode::All
}

fn default_report_url() -> String {
    // Fleet dashboard is opt-in; empty = off. Set a URL in Settings to enable.
    String::new()
}

fn default_schedule_time() -> String {
    "03:00".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        let mut components = BTreeMap::new();
        for name in COMPONENT_NAMES {
            components.insert((*name).to_string(), true);
        }
        AppConfig {
            scheduled_run_mode: RunMode::All,
            teams_webhook: String::new(),
            report_url: default_report_url(),
            schedule_enabled: false,
            schedule_time: default_schedule_time(),
            ha_url: String::new(),
            ha_token: String::new(),
            components,
        }
    }
}

impl AppConfig {
    pub fn enabled(&self, name: &str) -> bool {
        // Default to enabled if the key is absent.
        *self.components.get(name).unwrap_or(&true)
    }
}

pub fn load() -> AppConfig {
    let path = config_path();
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(mut cfg) = serde_json::from_str::<AppConfig>(&text) {
            // Ensure any newly-added components appear with a default of true.
            for name in COMPONENT_NAMES {
                cfg.components.entry((*name).to_string()).or_insert(true);
            }
            return cfg;
        }
    }
    let cfg = AppConfig::default();
    let _ = save(&cfg);
    cfg
}

pub fn save(cfg: &AppConfig) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(cfg).map_err(std::io::Error::other)?;
    std::fs::write(config_path(), text)
}
