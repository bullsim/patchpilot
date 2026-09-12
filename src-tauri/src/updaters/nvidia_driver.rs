//! Native NVIDIA GPU driver updater (Windows).
//!
//! Talks to NVIDIA's own driver catalogue — the same endpoints the nvidia.com
//! "Manual Driver Search" page uses — so the driver comes straight from NVIDIA
//! rather than via an OEM (Dell etc.) repackage that lags by weeks, and without
//! needing a third-party checker installed.
//!
//! Flow: nvidia-smi (GPU name + installed version) → series/product ids →
//! processFind (latest WHQL version) → predictable us.download.nvidia.com URL
//! (verified with a HEAD request) → silent install.

use crate::util::{kill_processes, run_cmd};
use std::path::PathBuf;
use std::time::Duration;

const UA: &str = "PatchPilot";
const LOOKUP: &str = "https://www.nvidia.com/Download/API/lookupValueSearch.aspx";
const FIND: &str = "https://www.nvidia.com/Download/processFind.aspx";
const CDN: &str = "https://us.download.nvidia.com/Windows";

/// NVIDIA osID values: Windows 11 = 135, Windows 10 64-bit = 57.
fn os_id(win11: bool) -> u32 {
    if win11 { 135 } else { 57 }
}

#[derive(Debug, Clone)]
pub struct Latest {
    pub version: String,
    pub kind: String, // "Game Ready" / "Studio"
    pub url: String,
    pub size_bytes: u64,
}

pub enum InstallOutcome {
    Installed,
    InstalledRebootRequired,
}

// ---------------------------------------------------------------- local GPU

/// (GPU name, installed driver version) from nvidia-smi.
pub async fn installed() -> Option<(String, String)> {
    let r = run_cmd(
        "nvidia-smi",
        &["--query-gpu=name,driver_version", "--format=csv,noheader"],
        Duration::from_secs(15),
    )
    .await;
    let line = r.stdout.lines().next()?.trim().to_string();
    let (name, ver) = line.rsplit_once(',')?;
    let (name, ver) = (name.trim(), ver.trim());
    if name.is_empty() || ver.is_empty() {
        return None;
    }
    Some((name.to_string(), ver.to_string()))
}

/// "616.92" -> (616, 92). Unknown formats compare as (0, 0).
pub fn parse_version(v: &str) -> (u32, u32) {
    let mut it = v.trim().split('.');
    let a = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let b = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (a, b)
}

pub fn is_newer(candidate: &str, installed: &str) -> bool {
    parse_version(candidate) > parse_version(installed)
}

// ---------------------------------------------------------------- HTTP

async fn http_get(url: &str) -> Result<String, String> {
    let r = run_cmd("curl", &["-sL", "-m", "40", "-A", UA, url], Duration::from_secs(45)).await;
    if r.code != Some(0) || r.stdout.trim().is_empty() {
        return Err(format!("request failed ({})", r.stderr.trim()));
    }
    Ok(r.stdout)
}

/// HEAD a URL; returns Content-Length when it answers 200.
async fn http_head_len(url: &str) -> Option<u64> {
    let r = run_cmd("curl", &["-sI", "-m", "30", "-A", UA, url], Duration::from_secs(35)).await;
    let head = r.stdout;
    let ok = head.lines().next().map(|l| l.contains(" 200")).unwrap_or(false);
    if !ok {
        return None;
    }
    head.lines()
        .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
        .and_then(|l| l.split(':').nth(1))
        .and_then(|v| v.trim().parse().ok())
}

// ---------------------------------------------------------------- catalogue

fn between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let i = s.find(start)? + start.len();
    let j = s[i..].find(end)? + i;
    Some(&s[i..j])
}

/// Parse NVIDIA's LookupValueSearch XML into (Name, Value) pairs.
fn parse_lookup(xml: &str) -> Vec<(String, String)> {
    xml.split("<LookupValue")
        .skip(1)
        .filter_map(|chunk| {
            let name = between(chunk, "<Name>", "</Name>")?;
            let value = between(chunk, "<Value>", "</Value>")?;
            Some((html_unescape(name), value.trim().to_string()))
        })
        .collect()
}

fn html_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&#160;", " ")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
}

/// Product name as NVIDIA lists it: "NVIDIA GeForce RTX 4070 Laptop GPU" ->
/// "GeForce RTX 4070 Laptop GPU".
fn product_name(gpu: &str) -> String {
    gpu.trim().trim_start_matches("NVIDIA").trim().to_string()
}

fn is_notebook(gpu: &str) -> bool {
    let g = gpu.to_ascii_lowercase();
    g.contains("laptop") || g.contains("notebook") || g.contains("max-q")
}

/// Generation token used in series names: "RTX 4070" -> "40", "GTX 1660" -> "16",
/// "GTX 980" -> "900" (NVIDIA names that "GeForce 900 Series").
fn generation(gpu: &str) -> Option<String> {
    let digits: String = gpu
        .split_whitespace()
        .find(|w| w.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false))
        .map(|w| w.chars().take_while(|c| c.is_ascii_digit()).collect())?;
    match digits.len() {
        4 => Some(digits[..2].to_string()),
        3 => Some(format!("{}00", &digits[..1])),
        _ => None,
    }
}

/// Resolve NVIDIA's (psid, pfid) for this GPU.
async fn resolve_ids(gpu: &str) -> Result<(String, String), String> {
    let wanted = product_name(gpu).to_ascii_lowercase();
    let notebook = is_notebook(gpu);
    let gen = generation(gpu);

    let series = parse_lookup(&http_get(&format!("{LOOKUP}?TypeID=2")).await?);
    if series.is_empty() {
        return Err("NVIDIA series list was empty".into());
    }

    // Narrow to GeForce series of the right form factor and generation first;
    // fall back to every GeForce series if nothing matches.
    let mut candidates: Vec<&(String, String)> = series
        .iter()
        .filter(|(n, _)| n.contains("GeForce"))
        .filter(|(n, _)| n.contains("(Notebooks)") == notebook)
        .filter(|(n, _)| gen.as_ref().map(|g| n.contains(&format!(" {g} Series"))).unwrap_or(true))
        .collect();
    if candidates.is_empty() {
        candidates = series.iter().filter(|(n, _)| n.contains("GeForce")).collect();
    }

    for (_, psid) in candidates.iter().take(12) {
        let products = parse_lookup(&http_get(&format!("{LOOKUP}?TypeID=3&ParentID={psid}")).await?);
        if let Some((_, pfid)) = products.iter().find(|(n, _)| n.to_ascii_lowercase() == wanted) {
            return Ok((psid.clone(), pfid.clone()));
        }
    }
    Err(format!("'{}' not found in NVIDIA's product list", product_name(gpu)))
}

/// Latest WHQL driver for the GPU (first row of NVIDIA's search results).
pub async fn latest(gpu: &str, win11: bool) -> Result<Latest, String> {
    let (psid, pfid) = resolve_ids(gpu).await?;
    let url = format!(
        "{FIND}?psid={psid}&pfid={pfid}&osid={}&lid=1&whql=1&lang=en-us&ctk=0&dtcid=1",
        os_id(win11)
    );
    let html = http_get(&url).await?;

    // Result rows are "<td class="gridItem">…</td>" cells: [icon, name, version, date…].
    let cells: Vec<String> = html
        .split("<td class=\"gridItem")
        .skip(1)
        .filter_map(|c| between(c, ">", "</td>"))
        .map(|c| html_unescape(&strip_tags(c)))
        .collect();

    let version = cells
        .iter()
        .find(|c| looks_like_version(c))
        .cloned()
        .ok_or("no driver version in NVIDIA's results")?;
    let kind = cells
        .iter()
        .find(|c| c.contains("Studio") || c.contains("Game Ready"))
        .map(|c| if c.contains("Studio") { "Studio" } else { "Game Ready" })
        .unwrap_or("Game Ready")
        .to_string();

    // NVIDIA's package URLs are predictable; verify with a HEAD before trusting one.
    let kinds: [&str; 2] = if is_notebook(gpu) { ["notebook", "desktop"] } else { ["desktop", "notebook"] };
    for k in kinds {
        let url = format!("{CDN}/{version}/{version}-{k}-win10-win11-64bit-international-dch-whql.exe");
        if let Some(len) = http_head_len(&url).await {
            if len > 200 * 1024 * 1024 {
                return Ok(Latest { version, kind, url, size_bytes: len });
            }
        }
    }
    Err(format!("driver {version} found but no downloadable package"))
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

fn looks_like_version(s: &str) -> bool {
    let s = s.trim();
    s.contains('.') && !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == '.')
}

// ---------------------------------------------------------------- install

fn download_dir() -> PathBuf {
    let d = std::env::temp_dir().join("PatchPilot");
    let _ = std::fs::create_dir_all(&d);
    d
}

/// Download the package and run NVIDIA's installer silently.
/// `progress` is called with a short status line.
pub async fn install(latest: &Latest, progress: impl Fn(&str)) -> Result<InstallOutcome, String> {
    let exe = download_dir().join(format!("nvidia-{}.exe", latest.version));
    let exe_s = exe.to_string_lossy().to_string();

    let need_download = std::fs::metadata(&exe).map(|m| m.len() != latest.size_bytes).unwrap_or(true);
    if need_download {
        progress(&format!(
            "Downloading driver {} ({} MB)…",
            latest.version,
            latest.size_bytes / (1024 * 1024)
        ));
        let r = run_cmd(
            "curl",
            &["-sL", "-m", "3600", "-A", UA, "-o", &exe_s, &latest.url],
            Duration::from_secs(3700),
        )
        .await;
        let got = std::fs::metadata(&exe).map(|m| m.len()).unwrap_or(0);
        if r.code != Some(0) || got != latest.size_bytes {
            let _ = std::fs::remove_file(&exe);
            return Err(format!(
                "download failed ({} of {} bytes){}",
                got,
                latest.size_bytes,
                if r.timed_out { ", timed out" } else { "" }
            ));
        }
    }

    // The installer wants the NVIDIA App / containers out of the way.
    kill_processes(&["NVIDIA App", "nvcontainer", "NVDisplay.Container", "NVIDIA Web Helper"]).await;
    tokio::time::sleep(Duration::from_secs(2)).await;

    progress(&format!("Installing driver {}… (screen may flicker)", latest.version));
    let r = run_cmd(&exe_s, &["-s", "-noreboot", "-noeula", "-nofinish"], Duration::from_secs(2400)).await;
    let _ = std::fs::remove_file(&exe);

    match r.code {
        Some(0) => Ok(InstallOutcome::Installed),
        // NVIDIA's setup returns 1 when the install succeeded but needs a reboot.
        Some(1) => Ok(InstallOutcome::InstalledRebootRequired),
        None if r.timed_out => Err("installer timed out".into()),
        Some(c) => Err(format!("installer exit code {c}")),
        None => Err(format!("installer did not start ({})", r.stderr.trim())),
    }
}

// ---------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parsing_and_comparison() {
        assert_eq!(parse_version("616.92"), (616, 92));
        assert!(is_newer("616.92", "596.08"));
        assert!(!is_newer("596.08", "596.08"));
        assert!(!is_newer("garbage", "596.08"));
    }

    #[test]
    fn gpu_name_helpers() {
        assert_eq!(product_name("NVIDIA GeForce RTX 4070 Laptop GPU"), "GeForce RTX 4070 Laptop GPU");
        assert!(is_notebook("NVIDIA GeForce RTX 4070 Laptop GPU"));
        assert!(!is_notebook("NVIDIA GeForce RTX 4090"));
        assert_eq!(generation("NVIDIA GeForce RTX 4070 Laptop GPU").as_deref(), Some("40"));
        assert_eq!(generation("NVIDIA GeForce GTX 1660 Ti").as_deref(), Some("16"));
        assert_eq!(generation("NVIDIA GeForce GTX 980").as_deref(), Some("900"));
    }

    #[test]
    fn lookup_xml_parses() {
        let xml = r#"<LookupValueSearch><LookupValues><LookupValue ParentID="129"><Name>GeForce RTX 4070 Laptop GPU</Name><Value>1006</Value></LookupValue></LookupValues></LookupValueSearch>"#;
        assert_eq!(parse_lookup(xml), vec![("GeForce RTX 4070 Laptop GPU".to_string(), "1006".to_string())]);
    }

    /// Hits nvidia.com. Run with: cargo test -- --ignored nvidia_live
    #[tokio::test]
    #[ignore]
    async fn nvidia_live_lookup() {
        let l = latest("NVIDIA GeForce RTX 4070 Laptop GPU", true).await.expect("lookup");
        println!("latest = {l:?}");
        assert!(parse_version(&l.version).0 >= 500);
        assert!(l.url.ends_with(".exe"));
    }
}
