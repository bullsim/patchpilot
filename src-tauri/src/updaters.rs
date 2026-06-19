//! Windows updaters — ported from daily_updates_v5.3.ps1.
//! Each updater reports via ctx.rep and may request a reboot.

use crate::model::Status;
use crate::orchestrator::Ctx;
use crate::util::{is_winget_ok, kill_processes, run_cmd, winget_result, CmdResult};
use std::path::Path;
use std::time::Duration;
use tokio::time::sleep;

/// Dispatch by component id.
pub async fn run(id: &str, ctx: &Ctx) {
    match id {
        "windows-update" => windows_update(ctx).await,
        "store" => store(ctx).await,
        "winget" => winget_all(ctx).await,
        "office" => office(ctx).await,
        "dell" => dell(ctx).await,
        "surface" => surface(ctx).await,
        "nvidia" => nvidia(ctx).await,
        "intel" => intel(ctx).await,
        "razer" => razer(ctx).await,
        "logitech" => logitech(ctx).await,
        "crucial" => crucial(ctx).await,
        other => ctx.rep.set(Status::Skipped, &format!("Unknown component '{other}'"), 0),
    }
}

const WINGET_FLAGS: &[&str] = &[
    "--silent",
    "--disable-interactivity",
    "--accept-source-agreements",
    "--accept-package-agreements",
];

async fn winget_upgrade(id: &str, secs: u64) -> CmdResult {
    let mut args = vec!["upgrade", "--id", id, "--exact"];
    args.extend_from_slice(WINGET_FLAGS);
    run_cmd("winget", &args, Duration::from_secs(secs)).await
}

// ---- 1. Windows Update ----
async fn windows_update(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Initiating scan…", 10);
    let uso = format!(
        "{}\\System32\\UsoClient.exe",
        std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into())
    );
    if !Path::new(&uso).exists() {
        ctx.rep.set(Status::Warning, "UsoClient.exe not found", 0);
        return;
    }
    for (i, action) in ["StartScan", "StartDownload", "StartInstall"].iter().enumerate() {
        if ctx.cancelled() {
            return;
        }
        run_cmd(&uso, &[action], Duration::from_secs(30)).await;
        ctx.rep.set(Status::Running, &format!("USOClient: {action}"), 30 + (i as i32) * 25);
        sleep(Duration::from_secs(2)).await;
    }
    ctx.rep.set(Status::Success, "Scan/Download/Install initiated", 100);
}

// ---- Microsoft Store (trigger update scan for all Store apps) ----
const STORE_PS: &str = r#"
$ns  = 'root\cimv2\mdm\dmmap'
$cls = 'MDM_EnterpriseModernAppManagement_AppManagement01'
try {
  $o = Get-CimInstance -Namespace $ns -ClassName $cls -ErrorAction Stop
  $r = Invoke-CimMethod -InputObject $o -MethodName UpdateScanMethod -ErrorAction Stop
  "RETURN:$($r.ReturnValue)"
} catch { "ERROR:$($_.Exception.Message)" }
"#;

async fn store(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Asking Microsoft Store to update apps…", 30);
    let res = run_cmd(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", STORE_PS],
        Duration::from_secs(180),
    )
    .await;
    let out = res.combined();
    if out.contains("RETURN:0") {
        ctx.rep.set(Status::Success, "Store update scan triggered", 100);
    } else if out.contains("RETURN:") {
        ctx.rep.set(Status::Warning, "Store scan returned non-zero", 50);
    } else {
        ctx.rep.set(Status::Warning, "Could not reach Store update service", 50);
    }
}

// ---- 2. Winget (all packages) ----
async fn winget_all(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Upgrading all packages…", 10);
    let mut args = vec!["upgrade", "--all", "--include-unknown"];
    args.extend_from_slice(WINGET_FLAGS);
    let res = run_cmd("winget", &args, Duration::from_secs(1800)).await;
    let ok = is_winget_ok(res.code) || res.combined().contains("No applicable upgrades");
    if ok {
        ctx.rep.set(Status::Success, "All packages processed", 100);
    } else {
        ctx.rep.set(Status::Warning, &format!("Exit: {:?}", res.code), 50);
    }
}

// ---- 3. Microsoft Office (Click-to-Run) ----
async fn office(ctx: &Ctx) {
    let c2r = "C:\\Program Files\\Common Files\\Microsoft Shared\\ClickToRun\\OfficeC2RClient.exe";
    if !Path::new(c2r).exists() {
        ctx.rep.set(Status::Skipped, "Click-to-Run not installed", 0);
        return;
    }
    ctx.rep.set(Status::Running, "Checking for updates…", 20);
    let res = run_cmd(
        c2r,
        &[
            "/update",
            "user",
            "displaylevel=false",
            "forceappshutdown=true",
            "updatepromptuser=false",
        ],
        Duration::from_secs(600),
    )
    .await;
    kill_processes(&["OfficeC2RClient"]).await;
    if res.timed_out {
        ctx.rep.set(Status::Warning, "Timed out (10 min)", 50);
    } else if res.code == Some(0) {
        ctx.rep.set(Status::Success, "Update completed", 100);
    } else {
        ctx.rep.set(Status::Warning, &format!("Exit: {:?}", res.code), 50);
    }
}

// ---- 5. Dell Stack (Command Update CLI) ----
async fn dell(ctx: &Ctx) {
    let probes = [
        "C:\\Program Files\\Dell\\CommandUpdate\\dcu-cli.exe",
        "C:\\Program Files (x86)\\Dell\\CommandUpdate\\dcu-cli.exe",
    ];
    let dcu = probes.iter().find(|p| Path::new(p).exists()).map(|s| s.to_string());

    // Only update what's already here — never install Dell Command Update.
    let Some(dcu) = dcu else {
        ctx.rep.set(Status::Skipped, "Dell Command Update not installed", 0);
        return;
    };

    ctx.rep.set(Status::Running, "Scanning BIOS/firmware…", 30);
    let scan = run_cmd(&dcu, &["/scan"], Duration::from_secs(600)).await;

    if scan.code == Some(5) {
        ctx.rep.set(Status::Warning, "Reboot required before updates", 50);
        ctx.rep.request_reboot();
        return;
    }
    let out = scan.combined();
    if out.contains("Number of applicable updates") && out.contains(": 0")
        || out.contains("No updates available")
    {
        ctx.rep.set(Status::Success, "No updates available", 100);
        return;
    }

    ctx.rep.set(Status::Running, "Applying updates…", 60);
    let apply = run_cmd(&dcu, &["/applyUpdates", "-silent"], Duration::from_secs(1800)).await;
    match apply.code {
        Some(0) => ctx.rep.set(Status::Success, "Updates applied", 100),
        Some(5) => {
            ctx.rep.set(Status::Warning, "Reboot required", 50);
            ctx.rep.request_reboot();
        }
        None if apply.timed_out => ctx.rep.set(Status::Warning, "Timed out", 50),
        None => ctx.rep.set(Status::Warning, "Dell CLI gave no result (needs admin)", 50),
        Some(c) => ctx.rep.set(Status::Warning, &format!("Exit code {c}"), 50),
    }
}

// ---- 6. Surface Stack ----
async fn surface(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Updating Surface App…", 30);
    let res = winget_upgrade("Microsoft.Surface", 600).await;
    let (st, detail) = winget_result(res.code, "Surface updated");
    ctx.rep.set(st, &detail, 100);
}

// ---- 7. Nvidia Stack ----
async fn nvidia(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Stopping NVIDIA processes…", 10);
    kill_processes(&[
        "NVIDIA App",
        "nvcontainer",
        "NVDisplay.Container",
        "NVIDIA Web Helper",
        "nvcplui",
        "nvsphelper64",
    ])
    .await;
    sleep(Duration::from_secs(3)).await;

    ctx.rep.set(Status::Running, "Updating NVIDIA App…", 30);
    let app = winget_upgrade("Nvidia.NVIDIAApp", 300).await;

    ctx.rep.set(Status::Running, "Updating NVIDIA drivers…", 70);
    let drv = winget_upgrade("Nvidia.GeForce.Experience", 600).await;

    let app_ok = is_winget_ok(app.code);
    let drv_ok = is_winget_ok(drv.code);
    if app_ok && drv_ok {
        ctx.rep.set(Status::Success, "NVIDIA App + drivers up to date", 100);
    } else if app_ok {
        ctx.rep.set(Status::Warning, &format!("App ok; driver exit {:?}", drv.code), 50);
    } else if drv_ok {
        ctx.rep.set(Status::Warning, &format!("Driver ok; app exit {:?}", app.code), 50);
    } else {
        ctx.rep.set(Status::Warning, &format!("App {:?} / Driver {:?}", app.code, drv.code), 50);
    }
}

// ---- 8. Intel GPU Stack ----
async fn intel(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Updating Intel DSA…", 30);
    let res = winget_upgrade("Intel.IntelDriverAndSupportAssistant", 600).await;
    let (st, detail) = winget_result(res.code, "Intel DSA updated");
    ctx.rep.set(st, &detail, 100);
}

// ---- 9. Razer Stack ----
async fn razer(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Updating Synapse 4…", 30);
    let res = winget_upgrade("RazerInc.RazerInstaller.Synapse4", 600).await;
    kill_processes(&["RazerInstaller", "Razer Synapse 4"]).await;
    let (st, detail) = winget_result(res.code, "Synapse 4 updated");
    ctx.rep.set(st, &detail, 100);
}

// ---- 10. Logitech Stack (user->machine scope migration) ----
async fn logitech(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Checking scope/update…", 30);
    let user = run_cmd("winget", &["list", "--id", "Logitech.GHUB", "--exact", "--scope", "user"], Duration::from_secs(40)).await;
    let mach = run_cmd("winget", &["list", "--id", "Logitech.GHUB", "--exact", "--scope", "machine"], Duration::from_secs(40)).await;

    let in_user = user.combined().contains("Logitech.GHUB");
    let in_machine = mach.combined().contains("Logitech.GHUB");

    let res = if in_user && !in_machine {
        ctx.rep.set(Status::Running, "Migrating user → machine scope…", 50);
        run_cmd("winget", &["uninstall", "--id", "Logitech.GHUB", "--exact", "--silent", "--scope", "user", "--force"], Duration::from_secs(300)).await;
        let mut args = vec!["install", "--id", "Logitech.GHUB", "--exact", "--scope", "machine"];
        args.extend_from_slice(WINGET_FLAGS);
        run_cmd("winget", &args, Duration::from_secs(600)).await
    } else {
        winget_upgrade("Logitech.GHUB", 600).await
    };
    let (st, detail) = winget_result(res.code, "G HUB updated");
    ctx.rep.set(st, &detail, 100);
}

// ---- 11. Crucial Stack ----
async fn crucial(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Updating Storage Executive…", 30);
    let res = winget_upgrade("Crucial.StorageExecutive", 600).await;
    let (st, detail) = winget_result(res.code, "Storage Executive updated");
    ctx.rep.set(st, &detail, 100);
}
