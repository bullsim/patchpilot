use crate::config::AppConfig;
use crate::model::{Category, RunMode};
use crate::system_info::SystemInfo;

#[derive(Clone, Copy)]
pub struct ComponentMeta {
    pub id: &'static str,
    pub name: &'static str,
    pub category: Category,
    pub applies: fn(&SystemInfo) -> bool,
}

#[allow(dead_code)]
fn always(_: &SystemInfo) -> bool {
    true
}

/// All components for the current OS, in run order.
#[cfg(windows)]
pub fn registry() -> Vec<ComponentMeta> {
    use Category::*;
    vec![
        ComponentMeta { id: "windows-update", name: "Windows Update",     category: Software, applies: always },
        ComponentMeta { id: "defender",       name: "Windows Defender",   category: Software, applies: always },
        ComponentMeta { id: "store",          name: "Microsoft Store",    category: Software, applies: always },
        ComponentMeta { id: "winget",         name: "Winget Packages",    category: Software, applies: always },
        ComponentMeta { id: "choco",          name: "Chocolatey",         category: Software, applies: |s| s.has_choco },
        ComponentMeta { id: "scoop",          name: "Scoop",              category: Software, applies: |s| s.has_scoop },
        ComponentMeta { id: "wsl",            name: "WSL",                category: Software, applies: |s| s.has_wsl },
        ComponentMeta { id: "office",         name: "Microsoft Office",   category: Software, applies: always },
        ComponentMeta { id: "dell",           name: "Dell Stack",         category: Firmware, applies: |s| s.is_dell },
        ComponentMeta { id: "surface",        name: "Surface Stack",      category: Firmware, applies: |s| s.is_surface },
        ComponentMeta { id: "nvidia",         name: "Nvidia Stack",       category: Firmware, applies: |s| s.has_nvidia },
        ComponentMeta { id: "intel",          name: "Intel GPU Stack",    category: Firmware, applies: |s| s.has_intel_gpu && s.app_intel_dsa },
        ComponentMeta { id: "razer",          name: "Razer Stack",        category: Software, applies: |s| s.app_razer },
        ComponentMeta { id: "logitech",       name: "Logitech Stack",     category: Software, applies: |s| s.app_logitech },
        ComponentMeta { id: "crucial",        name: "Crucial Stack",      category: Software, applies: |s| s.app_crucial },
        ComponentMeta { id: "homeassistant",  name: "Home Assistant",     category: Software, applies: always },
    ]
}

#[cfg(target_os = "macos")]
pub fn registry() -> Vec<ComponentMeta> {
    use Category::*;
    vec![
        ComponentMeta { id: "macos-update", name: "macOS Software Update", category: Firmware, applies: always },
        ComponentMeta { id: "brew",         name: "Homebrew",             category: Software, applies: |s| s.has_brew },
        ComponentMeta { id: "mas",          name: "Mac App Store",        category: Software, applies: |s| s.has_mas },
        ComponentMeta { id: "homeassistant", name: "Home Assistant",      category: Software, applies: always },
    ]
}

#[cfg(target_os = "linux")]
pub fn registry() -> Vec<ComponentMeta> {
    use Category::*;
    vec![
        ComponentMeta { id: "apt",     name: "APT Packages",      category: Software, applies: always },
        ComponentMeta { id: "flatpak", name: "Flatpak",           category: Software, applies: |s| s.has_flatpak },
        ComponentMeta { id: "snap",    name: "Snap",              category: Software, applies: |s| s.has_snap },
        ComponentMeta { id: "fwupd",   name: "Firmware (fwupd)",  category: Firmware, applies: |s| s.has_fwupd },
        ComponentMeta { id: "homeassistant", name: "Home Assistant", category: Software, applies: always },
    ]
}

/// Components that work on any OS (gated on the tool being installed).
fn cross_platform() -> Vec<ComponentMeta> {
    use Category::Software;
    vec![
        ComponentMeta { id: "rustup",       name: "Rust (rustup)", category: Software, applies: |s| s.has_rustup },
        ComponentMeta { id: "dotnet-tools", name: ".NET",          category: Software, applies: |s| s.has_dotnet },
        ComponentMeta { id: "npm-global",   name: "npm (global)",  category: Software, applies: |s| s.has_npm },
        ComponentMeta { id: "pip",          name: "Python (pip)",  category: Software, applies: |s| s.has_pip },
    ]
}

/// Look up a single component by id (across OS + cross-platform sets).
pub fn find(id: &str) -> Option<ComponentMeta> {
    registry().into_iter().chain(cross_platform()).find(|m| m.id == id)
}

/// Components that should run for this machine + mode + config.
pub fn selection(mode: RunMode, sys: &SystemInfo, cfg: &AppConfig) -> Vec<ComponentMeta> {
    registry()
        .into_iter()
        .chain(cross_platform())
        .filter(|m| mode.includes(m.category))
        .filter(|m| (m.applies)(sys))
        .filter(|m| cfg.enabled(m.name))
        // Home Assistant only appears once a URL is configured.
        .filter(|m| m.id != "homeassistant" || !cfg.ha_url.trim().is_empty())
        .collect()
}
