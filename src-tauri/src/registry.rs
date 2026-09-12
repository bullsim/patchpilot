use crate::config::AppConfig;
use crate::model::{Category, RunMode};
use crate::system_info::SystemInfo;

#[derive(Clone, Copy)]
pub struct ComponentMeta {
    pub id: &'static str,
    pub name: &'static str,
    pub category: Category,
    pub applies: fn(&SystemInfo) -> bool,
    /// Concurrency lane. Components in the same lane share a system resource
    /// (Windows Installer, the Windows Update service, the App Store daemon…)
    /// and run one after another; different lanes run at the same time.
    pub lane: &'static str,
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
        ComponentMeta { id: "windows-update", name: "Windows Update",     category: Software, applies: always, lane: "wu" },
        ComponentMeta { id: "defender",       name: "Windows Defender",   category: Software, applies: always, lane: "defender" },
        ComponentMeta { id: "store",          name: "Microsoft Store",    category: Software, applies: always, lane: "wu" },
        ComponentMeta { id: "winget",         name: "Winget Packages",    category: Software, applies: always, lane: "installer" },
        ComponentMeta { id: "choco",          name: "Chocolatey",         category: Software, applies: |s| s.has_choco, lane: "installer" },
        ComponentMeta { id: "scoop",          name: "Scoop",              category: Software, applies: |s| s.has_scoop, lane: "installer" },
        ComponentMeta { id: "wsl",            name: "WSL",                category: Software, applies: |s| s.has_wsl, lane: "installer" },
        ComponentMeta { id: "office",         name: "Microsoft Office",   category: Software, applies: always, lane: "office" },
        ComponentMeta { id: "nvidia",         name: "Nvidia Stack",       category: Firmware, applies: |s| s.has_nvidia, lane: "installer" },
        ComponentMeta { id: "dell",           name: "Dell Stack",         category: Firmware, applies: |s| s.is_dell, lane: "installer" },
        ComponentMeta { id: "surface",        name: "Surface Stack",      category: Firmware, applies: |s| s.is_surface, lane: "installer" },
        ComponentMeta { id: "intel",          name: "Intel GPU Stack",    category: Firmware, applies: |s| s.has_intel_gpu && s.app_intel_dsa, lane: "installer" },
        ComponentMeta { id: "razer",          name: "Razer Stack",        category: Software, applies: |s| s.app_razer, lane: "installer" },
        ComponentMeta { id: "logitech",       name: "Logitech Stack",     category: Software, applies: |s| s.app_logitech, lane: "installer" },
        ComponentMeta { id: "crucial",        name: "Crucial Stack",      category: Software, applies: |s| s.app_crucial, lane: "installer" },
        ComponentMeta { id: "homeassistant",  name: "Home Assistant",     category: Software, applies: always, lane: "ha" },
    ]
}

#[cfg(target_os = "macos")]
pub fn registry() -> Vec<ComponentMeta> {
    use Category::*;
    vec![
        ComponentMeta { id: "macos-update", name: "macOS Software Update", category: Firmware, applies: always, lane: "apple" },
        ComponentMeta { id: "brew",         name: "Homebrew",             category: Software, applies: |s| s.has_brew, lane: "brew" },
        ComponentMeta { id: "mas",          name: "Mac App Store",        category: Software, applies: |s| s.has_mas, lane: "apple" },
        ComponentMeta { id: "homeassistant", name: "Home Assistant",      category: Software, applies: always, lane: "ha" },
    ]
}

#[cfg(target_os = "linux")]
pub fn registry() -> Vec<ComponentMeta> {
    use Category::*;
    vec![
        ComponentMeta { id: "apt",     name: "APT Packages",      category: Software, applies: always, lane: "apt" },
        ComponentMeta { id: "flatpak", name: "Flatpak",           category: Software, applies: |s| s.has_flatpak, lane: "flatpak" },
        ComponentMeta { id: "snap",    name: "Snap",              category: Software, applies: |s| s.has_snap, lane: "snap" },
        ComponentMeta { id: "fwupd",   name: "Firmware (fwupd)",  category: Firmware, applies: |s| s.has_fwupd, lane: "fwupd" },
        ComponentMeta { id: "homeassistant", name: "Home Assistant", category: Software, applies: always, lane: "ha" },
    ]
}

/// Components that work on any OS (gated on the tool being installed).
fn cross_platform() -> Vec<ComponentMeta> {
    use Category::Software;
    vec![
        ComponentMeta { id: "rustup",       name: "Rust (rustup)", category: Software, applies: |s| s.has_rustup, lane: "rustup" },
        ComponentMeta { id: "dotnet-tools", name: ".NET",          category: Software, applies: |s| s.has_dotnet, lane: "installer" },
        ComponentMeta { id: "npm-global",   name: "npm (global)",  category: Software, applies: |s| s.has_npm, lane: "npm" },
        ComponentMeta { id: "pip",          name: "Python (pip)",  category: Software, applies: |s| s.has_pip, lane: "pip" },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_component_has_a_lane() {
        for m in registry().into_iter().chain(cross_platform()) {
            assert!(!m.lane.is_empty(), "{} has no lane", m.id);
        }
    }

    #[cfg(windows)]
    #[test]
    fn msi_based_components_share_the_installer_lane() {
        // Windows Installer only allows one install at a time; anything that
        // can invoke msiexec (winget and every vendor stack) must serialise.
        for id in [
            "winget", "choco", "scoop", "wsl", "dell", "surface", "nvidia", "intel",
            "razer", "logitech", "crucial", "dotnet-tools",
        ] {
            assert_eq!(find(id).unwrap().lane, "installer", "{id}");
        }
        assert_eq!(find("windows-update").unwrap().lane, find("store").unwrap().lane);
    }
}
