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
    // Windows
    pub is_dell: bool,
    pub is_surface: bool,
    pub has_nvidia: bool,
    pub has_intel_gpu: bool,
    pub app_razer: bool,
    pub app_logitech: bool,
    pub app_crucial: bool,
    pub app_intel_dsa: bool,
    // macOS
    pub has_brew: bool,
    pub has_mas: bool,
    // Linux
    pub has_flatpak: bool,
    pub has_snap: bool,
    pub has_fwupd: bool,
}

pub async fn detect() -> SystemInfo {
    let mut info = SystemInfo::default();
    #[cfg(windows)]
    detect_windows(&mut info).await;
    #[cfg(target_os = "macos")]
    detect_macos(&mut info).await;
    #[cfg(target_os = "linux")]
    detect_linux(&mut info).await;
    info
}

/// True if a CLI tool is on PATH (unix).
#[cfg(unix)]
async fn which(tool: &str) -> bool {
    run_cmd("sh", &["-c", &format!("command -v {tool}")], Duration::from_secs(10))
        .await
        .code
        == Some(0)
}

// ===================== Windows =====================

#[cfg(windows)]
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

#[cfg(windows)]
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

#[cfg(windows)]
async fn detect_windows(info: &mut SystemInfo) {
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
}

#[cfg(windows)]
async fn winget_installed(id: &str) -> bool {
    run_cmd(
        "winget",
        &["list", "--id", id, "--exact", "--accept-source-agreements"],
        Duration::from_secs(40),
    )
    .await
    .combined()
    .contains(id)
}

// ===================== macOS =====================

#[cfg(target_os = "macos")]
async fn detect_macos(info: &mut SystemInfo) {
    info.manufacturer = "Apple".into();
    let ver = run_cmd("sw_vers", &["-productVersion"], Duration::from_secs(15)).await;
    info.os = format!("macOS {}", ver.stdout.trim());
    let model = run_cmd("sysctl", &["-n", "hw.model"], Duration::from_secs(15)).await;
    info.model = model.stdout.trim().to_string();
    info.has_brew = which("brew").await;
    info.has_mas = which("mas").await;
}

// ===================== Linux =====================

#[cfg(target_os = "linux")]
async fn detect_linux(info: &mut SystemInfo) {
    // Distro name from /etc/os-release.
    info.os = std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("PRETTY_NAME="))
                .map(|l| l.trim_start_matches("PRETTY_NAME=").trim_matches('"').to_string())
        })
        .unwrap_or_else(|| "Linux".into());

    // Hardware: DMI (x86) or device-tree (Raspberry Pi / ARM SBCs).
    info.manufacturer = std::fs::read_to_string("/sys/devices/virtual/dmi/id/sys_vendor")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    info.model = std::fs::read_to_string("/sys/devices/virtual/dmi/id/product_name")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::fs::read_to_string("/proc/device-tree/model")
                .ok()
                .map(|s| s.trim_matches(char::from(0)).trim().to_string())
        })
        .unwrap_or_else(|| "Unknown".into());
    if info.manufacturer.is_empty() {
        info.manufacturer = if info.model.contains("Raspberry") { "Raspberry Pi".into() } else { "Unknown".into() };
    }

    info.has_flatpak = which("flatpak").await;
    info.has_snap = which("snap").await;
    info.has_fwupd = which("fwupdmgr").await;
}
