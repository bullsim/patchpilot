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
        "defender" => defender(ctx).await,
        "store" => store(ctx).await,
        "winget" => winget_all(ctx).await,
        "choco" => choco(ctx).await,
        "scoop" => scoop(ctx).await,
        "wsl" => wsl(ctx).await,
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

/// Dry-run: report available updates without applying. Only some components can
/// check non-destructively; the rest fall back to "run to update".
pub async fn check(id: &str, ctx: &Ctx) {
    match id {
        "winget" => winget_check(ctx).await,
        "windows-update" => windows_update_check(ctx).await,
        "choco" => choco_check(ctx).await,
        "nvidia" => nvidia_check(ctx).await,
        _ => super::no_check(ctx),
    }
}

/// Count the upgradable packages in `winget upgrade` output. Prefers the trailing
/// "N upgrades available." summary; falls back to counting table rows.
fn count_winget_upgrades(out: &str) -> usize {
    for line in out.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_suffix("upgrades available.") {
            if let Some(n) = rest.split_whitespace().last().and_then(|s| s.parse::<usize>().ok()) {
                return n;
            }
        }
    }
    // Fallback: rows after the dashed separator, until a blank/summary line.
    let mut counting = false;
    let mut n = 0;
    for line in out.lines() {
        let t = line.trim();
        if !counting {
            if t.len() > 10 && t.chars().all(|c| c == '-') {
                counting = true;
            }
            continue;
        }
        if t.is_empty() || t.contains("upgrades available") || t.contains("package(s)") {
            break;
        }
        n += 1;
    }
    n
}

async fn winget_check(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Checking winget for available upgrades…", 40);
    let res = run_cmd(
        "winget",
        &["upgrade", "--include-unknown", "--accept-source-agreements"],
        Duration::from_secs(180),
    )
    .await;
    super::report_count(ctx, count_winget_upgrades(&res.combined()), "package");
}

const WU_CHECK_PS: &str = r#"
try {
  $session  = New-Object -ComObject Microsoft.Update.Session
  $searcher = $session.CreateUpdateSearcher()
  $result   = $searcher.Search("IsInstalled=0 and IsHidden=0")
  Write-Output "COUNT:$($result.Updates.Count)"
} catch { Write-Output "ERROR:$($_.Exception.Message)" }
"#;

async fn windows_update_check(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Searching for Windows updates…", 40);
    let res = run_cmd(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", WU_CHECK_PS],
        Duration::from_secs(600),
    )
    .await;
    let out = res.combined();
    if let Some(line) = out.lines().map(str::trim).find(|l| l.starts_with("COUNT:")) {
        if let Ok(n) = line.trim_start_matches("COUNT:").trim().parse::<usize>() {
            return super::report_count(ctx, n, "update");
        }
    }
    ctx.rep.set(Status::Warning, "Couldn't query Windows Update (needs admin)", 50);
}

async fn choco_check(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Checking Chocolatey for outdated packages…", 40);
    // `choco outdated -r` prints one `name|current|available|pinned` line per package.
    let res = run_cmd("choco", &["outdated", "-r", "--ignore-pinned"], Duration::from_secs(180)).await;
    let n = res
        .stdout
        .lines()
        .map(str::trim)
        .filter(|l| l.contains('|'))
        .count();
    super::report_count(ctx, n, "package");
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

// ---- 1. Windows Update (COM API — actually downloads + installs + reports) ----
const WU_PS: &str = r#"
try {
  $session  = New-Object -ComObject Microsoft.Update.Session
  $searcher = $session.CreateUpdateSearcher()
  $result   = $searcher.Search("IsInstalled=0 and IsHidden=0")
  if ($result.Updates.Count -eq 0) { Write-Output 'RESULT:none'; exit 0 }
  $coll = New-Object -ComObject Microsoft.Update.UpdateColl
  foreach ($u in $result.Updates) { try { $u.AcceptEula() } catch {}; $coll.Add($u) | Out-Null }
  $dl = $session.CreateUpdateDownloader(); $dl.Updates = $coll; $dl.Download() | Out-Null
  $inst = $session.CreateUpdateInstaller(); $inst.Updates = $coll
  $r = $inst.Install()
  Write-Output "RESULT:$($r.ResultCode):$($coll.Count):$($r.RebootRequired)"
} catch { Write-Output "ERROR:$($_.Exception.Message)" }
"#;

async fn windows_update(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Searching + installing Windows updates…", 20);
    let res = run_cmd(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", WU_PS],
        Duration::from_secs(3600),
    )
    .await;
    let out = res.combined();
    let line = out.lines().map(str::trim).find(|l| l.starts_with("RESULT:") || l.starts_with("ERROR:"));
    match line {
        Some("RESULT:none") => ctx.rep.set(Status::Success, "Already up to date", 100),
        Some(l) if l.starts_with("RESULT:") => {
            let p: Vec<&str> = l.trim_start_matches("RESULT:").split(':').collect();
            let code = p.first().copied().unwrap_or("");
            let count = p.get(1).copied().unwrap_or("?");
            let reboot = p.get(2).map(|s| s.eq_ignore_ascii_case("true")).unwrap_or(false);
            // ResultCode 2 = Succeeded, 3 = Succeeded with errors.
            if code == "2" || code == "3" {
                if reboot {
                    ctx.rep.set(Status::Warning, &format!("{count} update(s) installed — reboot required"), 60);
                    ctx.rep.request_reboot();
                } else {
                    ctx.rep.set(Status::Success, &format!("{count} update(s) installed"), 100);
                }
            } else {
                ctx.rep.set(Status::Warning, &format!("Install result code {code}"), 50);
            }
        }
        _ => ctx.rep.set(Status::Warning, "Windows Update failed (needs admin)", 50),
    }
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

// ---- Windows Defender (signature update) ----
async fn defender(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Updating Defender signatures…", 40);
    let res = run_cmd(
        "powershell",
        &["-NoProfile", "-Command", "Update-MpSignature; exit $LASTEXITCODE"],
        Duration::from_secs(300),
    )
    .await;
    match res.code {
        Some(0) => ctx.rep.set(Status::Success, "Signatures up to date", 100),
        _ => ctx.rep.set(Status::Warning, "Couldn't update signatures (3rd-party AV?)", 50),
    }
}

// ---- Chocolatey (all packages; needs admin) ----
async fn choco(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "choco upgrade all…", 30);
    let res = run_cmd(
        "choco",
        &["upgrade", "all", "-y", "--no-progress"],
        Duration::from_secs(2400),
    )
    .await;
    match res.code {
        Some(0) => ctx.rep.set(Status::Success, "Chocolatey packages upgraded", 100),
        Some(1641) | Some(3010) => {
            ctx.rep.set(Status::Warning, "Upgraded — reboot pending", 60);
            ctx.rep.request_reboot();
        }
        c => ctx.rep.set(Status::Warning, &format!("Exit: {c:?}"), 50),
    }
}

// ---- Scoop (user-level) ----
async fn scoop(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Updating Scoop + buckets…", 30);
    run_cmd("scoop", &["update"], Duration::from_secs(600)).await;
    if ctx.cancelled() {
        return;
    }
    ctx.rep.set(Status::Running, "scoop update *…", 70);
    let res = run_cmd("scoop", &["update", "*"], Duration::from_secs(2400)).await;
    match res.code {
        Some(0) => ctx.rep.set(Status::Success, "Scoop apps updated", 100),
        c => ctx.rep.set(Status::Warning, &format!("Exit: {c:?}"), 50),
    }
}

// ---- WSL (kernel/components) ----
async fn wsl(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "wsl --update…", 40);
    let res = run_cmd("wsl", &["--update"], Duration::from_secs(900)).await;
    match res.code {
        Some(0) => ctx.rep.set(Status::Success, "WSL up to date", 100),
        c => ctx.rep.set(Status::Warning, &format!("Exit: {c:?}"), 50),
    }
}

// ---- 2. Winget (all packages) ----
async fn winget_all(ctx: &Ctx) {
    ctx.rep.set(Status::Running, "Refreshing winget sources…", 8);
    run_cmd("winget", &["source", "update"], Duration::from_secs(120)).await;
    // Pin excluded packages so `upgrade --all` skips them.
    for id in &ctx.winget_excludes {
        let id = id.trim();
        if !id.is_empty() {
            run_cmd("winget", &["pin", "add", "--id", id, "--exact"], Duration::from_secs(30)).await;
        }
    }
    ctx.rep.set(Status::Running, "Upgrading all packages…", 15);
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
/// Human-readable meaning of a Dell Command | Update CLI (dcu-cli.exe) exit code.
fn dcu_message(code: i32) -> Option<&'static str> {
    Some(match code {
        0 => "Success",
        1 => "Reboot required",
        2 => "Dell Command Update reported an unknown error",
        3 => "Not a Dell system",
        4 => "Dell CLI needs administrator rights",
        5 => "Reboot pending from a previous update",
        6 => "Dell Command Update app is already running",
        7 | 8 => "System not supported by Dell Command Update",
        500 => "No updates available",
        501 => "Dell couldn't determine applicable updates",
        502 => "Dell update was cancelled",
        503 => "Dell couldn't download updates",
        _ => return None,
    })
}

/// Parse "Number of applicable updates for the current system configuration: N".
fn dcu_applicable_count(out: &str) -> Option<u32> {
    out.lines()
        .find(|l| l.contains("Number of applicable updates"))
        .and_then(|l| l.rsplit(':').next())
        .and_then(|n| n.trim().parse().ok())
}

fn dcu_detail(code: i32) -> String {
    dcu_message(code)
        .map(str::to_string)
        .unwrap_or_else(|| format!("Dell exit code {code}"))
}

/// Run dcu-cli. Exit code 6 means the Dell Command Update desktop app (or its
/// own scheduled scan) holds the lock; close it and retry once.
async fn dcu_run(dcu: &str, args: &[&str], secs: u64) -> crate::util::CmdResult {
    let mut res = run_cmd(dcu, args, Duration::from_secs(secs)).await;
    if res.code == Some(6) {
        kill_processes(&["DellCommandUpdate"]).await;
        tokio::time::sleep(Duration::from_secs(5)).await;
        res = run_cmd(dcu, args, Duration::from_secs(secs)).await;
    }
    res
}

async fn dell(ctx: &Ctx) {
    let probes = [
        r"C:\Program Files\Dell\CommandUpdate\dcu-cli.exe",
        r"C:\Program Files (x86)\Dell\CommandUpdate\dcu-cli.exe",
    ];
    let dcu = probes.iter().find(|p| Path::new(p).exists()).map(|s| s.to_string());

    // Only update what's already here — never install Dell Command Update.
    let Some(dcu) = dcu else {
        ctx.rep.set(Status::Skipped, "Dell Command Update not installed", 0);
        return;
    };

    // The DCU desktop app blocks the CLI ("another instance is running").
    kill_processes(&["DellCommandUpdate"]).await;

    ctx.rep.set(Status::Running, "Scanning BIOS/firmware…", 30);
    let scan = dcu_run(&dcu, &["/scan"], 600).await;

    match scan.code {
        Some(5) => {
            ctx.rep.set(Status::Warning, "Reboot required before updates", 50);
            ctx.rep.request_reboot();
            return;
        }
        Some(500) => {
            ctx.rep.set(Status::Success, "No updates available", 100);
            return;
        }
        Some(c) if c != 0 && c != 1 => {
            ctx.rep.set(Status::Warning, &dcu_detail(c), 50);
            return;
        }
        None if scan.timed_out => {
            ctx.rep.set(Status::Warning, "Dell scan timed out", 50);
            return;
        }
        _ => {}
    }
    let out = scan.combined();
    let count = dcu_applicable_count(&out);
    if count == Some(0) || out.contains("No updates available") {
        ctx.rep.set(Status::Success, "No updates available", 100);
        return;
    }

    let label = match count {
        Some(n) => format!("Applying {n} update(s)…"),
        None => "Applying updates…".to_string(),
    };
    ctx.rep.set(Status::Running, &label, 60);
    let apply = dcu_run(&dcu, &["/applyUpdates", "-silent"], 1800).await;
    match apply.code {
        Some(0) => ctx.rep.set(
            Status::Success,
            &match count { Some(n) => format!("{n} update(s) applied"), None => "Updates applied".into() },
            100,
        ),
        // 1 = the operation needs a reboot to finish; 5 = a reboot was already pending.
        Some(1) | Some(5) => {
            ctx.rep.set(Status::Warning, "Updates applied — reboot required", 50);
            ctx.rep.request_reboot();
        }
        Some(500) => ctx.rep.set(Status::Success, "No updates available", 100),
        None if apply.timed_out => ctx.rep.set(Status::Warning, "Timed out", 50),
        None => ctx.rep.set(Status::Warning, "Dell CLI gave no result (needs admin)", 50),
        Some(c) => ctx.rep.set(Status::Warning, &dcu_detail(c), 50),
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
// winget only updates the NVIDIA App shell. The GPU driver itself comes straight
// from NVIDIA's catalogue (nvidia_driver.rs): look up the latest WHQL package for
// this GPU, and silently install it if it is newer than what nvidia-smi reports.
// TinyNvidiaUpdateChecker is only used as a fallback if the lookup fails.
async fn nvidia(ctx: &Ctx) {
    use super::nvidia_driver::{self as nv, InstallOutcome};

    ctx.rep.set(Status::Running, "Updating NVIDIA App…", 20);
    let app = winget_upgrade("Nvidia.NVIDIAApp", 300).await;
    let app_ok = is_winget_ok(app.code);
    let app_txt = if app_ok { "NVIDIA App up to date" } else { "NVIDIA App update failed" };
    let app_status = if app_ok { Status::Success } else { Status::Warning };

    ctx.rep.set(Status::Running, "Checking NVIDIA for a newer GPU driver…", 40);
    let Some((gpu, installed)) = nv::installed().await else {
        ctx.rep.set(app_status, &format!("{app_txt} · nvidia-smi unavailable, GPU driver not checked"), 100);
        return;
    };
    let win11 = ctx.sys.os.contains("11");

    match nv::latest(&gpu, win11).await {
        Ok(latest) if nv::is_newer(&latest.version, &installed) => {
            if ctx.cancelled() {
                ctx.rep.set(Status::Skipped, "Cancelled", 100);
                return;
            }
            let rep = &ctx.rep;
            let v = latest.version.clone();
            let kind = latest.kind.clone();
            match nv::install(&latest, |m| rep.set(Status::Running, m, 70)).await {
                Ok(InstallOutcome::Installed) => ctx.rep.set(
                    Status::Success,
                    &format!("{app_txt} · GPU driver {installed} → {v} ({kind}) installed"),
                    100,
                ),
                Ok(InstallOutcome::InstalledRebootRequired) => {
                    ctx.rep.set(
                        Status::Warning,
                        &format!("{app_txt} · GPU driver {installed} → {v} ({kind}) installed — reboot required"),
                        60,
                    );
                    ctx.rep.request_reboot();
                }
                Err(e) => ctx.rep.set(
                    Status::Warning,
                    &format!("{app_txt} · GPU driver {v} available but install failed: {e}"),
                    60,
                ),
            }
        }
        Ok(latest) => ctx.rep.set(
            app_status,
            &format!("{app_txt} · GPU driver {installed} is current ({})", latest.kind),
            100,
        ),
        Err(e) => {
            // Catalogue lookup failed (offline, or an unusual GPU name). Fall back
            // to TinyNvidiaUpdateChecker if the user has it; otherwise say so plainly.
            let Some(tnuc) = locate_tnuc().await else {
                ctx.rep.set(
                    app_status,
                    &format!("{app_txt} · GPU driver {installed} installed; NVIDIA lookup failed ({e})"),
                    100,
                );
                return;
            };
            ctx.rep.set(Status::Running, "Installing latest GPU driver (TinyNvidiaUpdateChecker)…", 70);
            kill_processes(&["NVIDIA App", "nvcontainer", "NVDisplay.Container", "NVIDIA Web Helper"]).await;
            sleep(Duration::from_secs(2)).await;
            let drv = run_cmd(&tnuc, &["--quiet", "--no-prompt"], Duration::from_secs(2400)).await;
            match drv.code {
                Some(0) => ctx.rep.set(Status::Success, "NVIDIA App + GPU driver up to date", 100),
                None if drv.timed_out => ctx.rep.set(Status::Warning, "Driver install timed out", 60),
                c => ctx.rep.set(Status::Warning, &format!("App ok; driver checker exit {c:?}"), 60),
            }
        }
    }
}

/// Dry-run: compare the installed GPU driver with NVIDIA's latest, change nothing.
async fn nvidia_check(ctx: &Ctx) {
    use super::nvidia_driver as nv;
    ctx.rep.set(Status::Running, "Checking NVIDIA…", 40);
    let Some((gpu, installed)) = nv::installed().await else {
        return super::no_check(ctx);
    };
    match nv::latest(&gpu, ctx.sys.os.contains("11")).await {
        Ok(l) if nv::is_newer(&l.version, &installed) => ctx.rep.set(
            Status::Warning,
            &format!("GPU driver {installed} → {} ({}) available", l.version, l.kind),
            100,
        ),
        Ok(l) => ctx.rep.set(Status::Success, &format!("GPU driver {installed} is current ({})", l.kind), 100),
        Err(e) => ctx.rep.set(Status::Skipped, &format!("Couldn't query NVIDIA ({e})"), 100),
    }
}

async fn locate_tnuc() -> Option<String> {
    let w = run_cmd("where", &["TinyNvidiaUpdateChecker.exe"], Duration::from_secs(15)).await;
    if w.code == Some(0) {
        if let Some(line) = w.stdout.lines().next() {
            let p = line.trim();
            if !p.is_empty() {
                return Some(p.to_string());
            }
        }
    }
    None
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
