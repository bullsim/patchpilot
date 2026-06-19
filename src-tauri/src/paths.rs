use std::path::PathBuf;

/// Per-user app data directory (created if missing).
pub fn app_dir() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir())
    } else {
        std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .unwrap_or_else(|_| std::env::temp_dir())
    };
    let dir = base.join("PatchPilot");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn config_path() -> PathBuf {
    app_dir().join("config.json")
}

pub fn reboot_path() -> PathBuf {
    app_dir().join("reboot.json")
}

pub fn logs_dir() -> PathBuf {
    let d = app_dir().join("logs");
    let _ = std::fs::create_dir_all(&d);
    d
}
