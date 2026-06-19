use crate::util::run_cmd;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub manufacturer: String,
    pub model: String,
    pub gpus: String,
    pub os: String,
    pub is_dell: bool,
    pub is_surface: bool,
    pub has_nvidia: bool,
    pub has_intel_gpu: bool,
    pub app_razer: bool,
    pub app_logitech: bool,
    pub app_crucial: bool,
    pub app_intel_dsa: bool,
}

#[derive(Deserialize)]
struct RawInfo {
    #[serde(rename = "Manufacturer")]
    manufacturer: Option<String>,
    #[serde(rename = "Model")]
    model: Option<String>,
    #[serde(rename = "SystemFamily")]
    system_family: Option<String>,
    #[serde(rename = "GPUs")]
    gpus: Option<String>,
    #[serde(rename = "OS")]
    os: Option<String>,
}

const PS_QUERY: &str = r#"
$cs = Get-CimInstance Win32_ComputerSystem
$gpus = (Get-CimInstance Win32_VideoController | ForEach-Object { $_.Name }) -join ', '
[pscustomobject]@{
  Manufacturer = $cs.Manufacturer
  Model        = $cs.Model
  SystemFamily = $cs.SystemFamily
  GPUs         = $gpus
  OS           = [System.Environment]::OSVersion.VersionString
} | ConvertTo-Json -Compress
"#;

pub async fn detect() -> SystemInfo {
    let mut info = SystemInfo::default();

    if cfg!(windows) {
        let res = run_cmd(
            "powershell",
            &["-NoProfile", "-NonInteractive", "-Command", PS_QUERY],
            Duration::from_secs(30),
        )
        .await;

        if let Ok(raw) = serde_json::from_str::<RawInfo>(res.stdout.trim()) {
            let mfr = raw.manufacturer.unwrap_or_default().trim().to_string();
            let model = raw.model.unwrap_or_default().trim().to_string();
            let fam = raw.system_family.unwrap_or_default();
            let gpus = raw.gpus.unwrap_or_default();

            info.is_dell = mfr.to_lowercase().starts_with("dell");
            info.is_surface = mfr.to_lowercase().starts_with("microsoft")
                && (model.to_lowercase().starts_with("surface")
                    || fam.to_lowercase().starts_with("surface"));

            let g = gpus.to_lowercase();
            info.has_nvidia = g.contains("nvidia");
            let has_amd = g.contains("amd") && !info.has_nvidia;
            info.has_intel_gpu = g.contains("intel") && !info.has_nvidia && !has_amd;

            info.manufacturer = mfr;
            info.model = model;
            info.gpus = gpus;
            info.os = raw.os.unwrap_or_else(|| "Windows".into());
        } else {
            info.manufacturer = "Unknown".into();
            info.model = "Unknown".into();
            info.os = "Windows".into();
        }

        info.app_razer = winget_installed("RazerInc.RazerInstaller.Synapse4").await;
        info.app_logitech = winget_installed("Logitech.GHUB").await;
        info.app_crucial = winget_installed("Crucial.StorageExecutive").await;
        info.app_intel_dsa = winget_installed("Intel.IntelDriverAndSupportAssistant").await;
    } else {
        // mac/linux backends arrive in later phases.
        info.os = std::env::consts::OS.to_string();
        info.manufacturer = "Unknown".into();
        info.model = "Unknown".into();
    }

    info
}

/// True if winget reports the package id as installed.
pub async fn winget_installed(id: &str) -> bool {
    let res = run_cmd(
        "winget",
        &["list", "--id", id, "--exact", "--accept-source-agreements"],
        Duration::from_secs(40),
    )
    .await;
    res.combined().contains(id)
}
