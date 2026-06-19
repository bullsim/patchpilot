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

fn always(_: &SystemInfo) -> bool {
    true
}

/// All components, in run order (matches v5.3).
pub fn registry() -> Vec<ComponentMeta> {
    use Category::*;
    vec![
        ComponentMeta { id: "windows-update", name: "Windows Update",     category: Software, applies: always },
        ComponentMeta { id: "store",          name: "Microsoft Store",    category: Software, applies: always },
        ComponentMeta { id: "winget",         name: "Winget Packages",    category: Software, applies: always },
        ComponentMeta { id: "office",         name: "Microsoft Office",   category: Software, applies: always },
        ComponentMeta { id: "dell",           name: "Dell Stack",         category: Firmware, applies: |s| s.is_dell },
        ComponentMeta { id: "surface",        name: "Surface Stack",      category: Firmware, applies: |s| s.is_surface },
        ComponentMeta { id: "nvidia",         name: "Nvidia Stack",       category: Firmware, applies: |s| s.has_nvidia },
        ComponentMeta { id: "intel",          name: "Intel GPU Stack",    category: Firmware, applies: |s| s.has_intel_gpu && s.app_intel_dsa },
        ComponentMeta { id: "razer",          name: "Razer Stack",        category: Software, applies: |s| s.app_razer },
        ComponentMeta { id: "logitech",       name: "Logitech Stack",     category: Software, applies: |s| s.app_logitech },
        ComponentMeta { id: "crucial",        name: "Crucial Stack",      category: Software, applies: |s| s.app_crucial },
    ]
}

/// Components that should run for this machine + mode + config.
pub fn selection(mode: RunMode, sys: &SystemInfo, cfg: &AppConfig) -> Vec<ComponentMeta> {
    registry()
        .into_iter()
        .filter(|m| mode.includes(m.category))
        .filter(|m| (m.applies)(sys))
        .filter(|m| cfg.enabled(m.name))
        .collect()
}
