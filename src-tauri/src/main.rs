// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Scheduled headless runs are launched already-elevated by Task Scheduler.
    if args.iter().any(|a| a == "--silent") {
        patchpilot_lib::run_silent(&args);
        return;
    }

    // Updates (Dell firmware, Microsoft Store, etc.) require administrator rights.
    // If we're not elevated, relaunch ourselves elevated (one UAC prompt) and exit.
    #[cfg(windows)]
    if !args.iter().any(|a| a == "--no-elevate") && !is_elevated() && elevate() {
        return;
    }

    patchpilot_lib::run();
}

/// True if the current process has an elevated (admin) token.
#[cfg(windows)]
fn is_elevated() -> bool {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    // `net session` only succeeds with admin rights.
    Command::new("net")
        .args(["session"])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Relaunch this exe elevated via the UAC "runas" verb.
/// Returns true if the elevated instance was launched (then the caller should exit).
#[cfg(windows)]
fn elevate() -> bool {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let path = exe.display().to_string().replace('\'', "''");
    let ps = format!(
        "try {{ Start-Process -FilePath '{path}' -ArgumentList '--no-elevate' -Verb RunAs; exit 0 }} catch {{ exit 1 }}"
    );
    Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps])
        .creation_flags(0x0800_0000)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
