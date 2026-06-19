use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Status {
    Pending,
    Running,
    Success,
    Warning,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Category {
    Software,
    Firmware,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RunMode {
    All,
    Software,
    Firmware,
}

impl RunMode {
    pub fn from_str(s: &str) -> RunMode {
        match s.to_ascii_lowercase().as_str() {
            "software" => RunMode::Software,
            "firmware" => RunMode::Firmware,
            _ => RunMode::All,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            RunMode::All => "All",
            RunMode::Software => "Software",
            RunMode::Firmware => "Firmware",
        }
    }
    pub fn includes(&self, cat: Category) -> bool {
        match self {
            RunMode::All => true,
            RunMode::Software => cat == Category::Software,
            RunMode::Firmware => cat == Category::Firmware,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentStatus {
    pub id: String,
    pub name: String,
    pub category: Category,
    pub status: Status,
    pub detail: String,
    pub progress: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub mode: RunMode,
    pub ok: u32,
    pub warn: u32,
    pub fail: u32,
    pub skip: u32,
    pub duration_secs: u64,
    pub reboot_required: bool,
}
