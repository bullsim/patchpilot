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
    if !args.iter().any(|a| a == "--no-elevate")
        && !patchpilot_lib::is_elevated()
        && patchpilot_lib::elevate()
    {
        return;
    }

    patchpilot_lib::run();
}
