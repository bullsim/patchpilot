use crate::model::Status;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct CmdResult {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

impl CmdResult {
    pub fn combined(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
    }
}

/// Run a process with a hard timeout. On timeout it is killed and `timed_out` is set.
pub async fn run_cmd(program: &str, args: &[&str], to: Duration) -> CmdResult {
    let mut cmd = Command::new(program);
    cmd.args(args);
    #[cfg(windows)]
    {
        // tokio's Command exposes creation_flags directly on Windows.
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return CmdResult {
                code: None,
                stdout: String::new(),
                stderr: format!("spawn failed: {e}"),
                timed_out: false,
            }
        }
    };

    match timeout(to, child.wait_with_output()).await {
        Ok(Ok(out)) => CmdResult {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).to_string(),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
            timed_out: false,
        },
        Ok(Err(e)) => CmdResult {
            code: None,
            stdout: String::new(),
            stderr: format!("io error: {e}"),
            timed_out: false,
        },
        Err(_) => CmdResult {
            code: None,
            stdout: String::new(),
            stderr: "timed out".into(),
            timed_out: true,
        },
    }
}

/// Map a winget/MSI-style exit code to a status + human detail.
/// Ported from v5.3 `Get-WingetResult`.
pub fn winget_result(code: Option<i32>, action: &str) -> (Status, String) {
    match code {
        Some(0) => (Status::Success, format!("{action} successfully")),
        Some(-1978335189) => (Status::Success, "Already up to date".into()),
        Some(-1978335212) => (Status::Success, "Already at required version".into()),
        Some(-1978335188) => (Status::Success, "Not installed (nothing to do)".into()),
        None => (Status::Warning, "Timed out".into()),
        Some(c) => (Status::Warning, format!("Exit code: {c}")),
    }
}

/// Whether a winget exit code counts as "ok / no action needed".
pub fn is_winget_ok(code: Option<i32>) -> bool {
    matches!(code, Some(0) | Some(-1978335189) | Some(-1978335212) | Some(-1978335188))
}

/// Kill processes by image name (best effort), e.g. for orphan cleanup.
pub async fn kill_processes(names: &[&str]) {
    for n in names {
        let image = if n.to_ascii_lowercase().ends_with(".exe") {
            n.to_string()
        } else {
            format!("{n}.exe")
        };
        let _ = run_cmd("taskkill", &["/F", "/IM", &image, "/T"], Duration::from_secs(15)).await;
    }
}
