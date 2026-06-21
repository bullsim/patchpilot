# PatchPilot

A slick cross-platform desktop app that keeps your machines up to date — Dell, Surface,
Mac, Zorin, Raspberry Pi. The same proven engine as `daily_updates_v5.3.ps1`, rebuilt as a
real app with a live UI, per-OS detection, and signed self-update.

What it updates (only what's already installed — see the principle below):

- **Windows:** Windows Update, Defender signatures, Microsoft Store, winget, Chocolatey,
  Scoop, WSL, Office C2R, Dell Command Update, Surface, Nvidia (via TinyNvidiaUpdateChecker),
  Intel, Razer, Logitech, Crucial.
- **Cross-platform:** Home Assistant (Core/OS/add-ons via its API).
- Runs from the **system tray** (close hides to tray; "Run all updates" + Quit from the menu).
- **macOS:** `softwareupdate`, `brew`, `mas`.
- **Linux / Zorin / Raspberry Pi:** `apt`, `flatpak`, `snap`, `fwupdmgr`.

> **Principle: update what's installed, never install new software.** PatchPilot only
> upgrades apps/firmware already present on the machine; anything not installed is skipped,
> not installed.

## ⬇️ Download & install

Grab the latest build for each machine from the **Releases** page:

### **https://github.com/bullsim/patchpilot/releases/latest**

| Machine | File to download |
|---|---|
| Windows (Dell, Surface, x64) | `PatchPilot_*_x64-setup.exe` |
| macOS (Intel + Apple Silicon) | `PatchPilot_*_universal.dmg` |
| Linux x64 (Zorin, Ubuntu) | `PatchPilot_*_amd64.deb` or `*_amd64.AppImage` |
| Raspberry Pi / ARM64 Linux | `PatchPilot_*_arm64.deb` or `*_aarch64.AppImage` |

Install once per machine — after that the app **auto-updates itself** from new releases.
No build tools needed on the target machines; only this dev machine builds.

## Stack

- **Tauri v2** shell · **React + TypeScript** UI (Vite) · **Rust** backend.
- Live status via Tauri events (no polling). Per-component config, run modes
  (All / Software / Firmware), reboot scheduling, logs, and a headless scheduled mode.

## Architecture (where things live)

```
src/                       React UI
  App.tsx                  shell: header, run buttons, progress, card grid, reboot banner
  components/Card.tsx      one status card
  components/RebootBanner  Now / Tonight 02:00 / Custom / Cancel + 5-min auto
  components/SettingsPanel per-component toggles, scheduled mode, Teams webhook
  lib/api.ts               invoke() wrappers + event listeners
  lib/types.ts             types mirroring the Rust structs

src-tauri/src/
  model.rs                 Status / Category / RunMode / ComponentStatus / RunSummary
  system_info.rs           hardware + OS detection (Dell/Surface/Nvidia/Intel/apps)
  config.rs                load/save config.json (per-OS component names)
  registry.rs              ComponentMeta + applies() + mode/config selection (per OS)
  orchestrator.rs          Reporter trait, sequencing, counts, summary, reboot flag
  updaters/               per-OS backends: windows.rs / macos.rs / linux.rs
  reboot.rs                schedule / cancel / query reboot (schtasks)
  util.rs                  run-with-timeout, winget exit-code mapping, process kill
  paths.rs                 per-user app-data dirs (config, logs, reboot marker)
  lib.rs                   Tauri commands + events + silent CLI entry
  main.rs                  GUI vs --silent dispatch
```

App data lives in `%LOCALAPPDATA%\PatchPilot\` (`config.json`, `logs/`, `reboot.json`).

## Prerequisites

- [Rust](https://rustup.rs) (MSVC toolchain) + Visual Studio C++ Build Tools
- [Node.js](https://nodejs.org) 18+
- WebView2 (ships with Windows 11)

## Develop

```bash
npm install
npm run tauri dev      # hot-reload UI + Rust
```

## Build the installer

```bash
npm run tauri build
# -> src-tauri/target/release/bundle/nsis/PatchPilot_x.y.z_x64-setup.exe
```

All-platform installers are built by CI on every version tag (see "Cutting a release").

## Scheduled (headless) runs

**Easiest — in the app:** open **Settings** → tick **"Run automatically every day"**, pick a
time and run mode, **Save**. PatchPilot registers the OS task for you (Windows Task Scheduler /
macOS launchd / Linux user systemd timer) and keeps it in sync. On Windows the task runs
elevated, so updates apply with no prompt.

The app can also run headless directly:

```bash
patchpilot.exe --silent --mode All        # or Software / Firmware
```

Or register a daily task manually with the helper scripts:

```powershell
# Windows (elevated PowerShell)
.\install-scheduled-task.ps1 -Time 03:00 -Mode All
```
```bash
# macOS (launchd, runs in your session)
./install-schedule-macos.sh 3 0 All
# Linux / Raspberry Pi (systemd timer, root)
sudo ./install-schedule-linux.sh 03:00 All
```

All three invoke the same `--silent --mode` headless path. Updates require admin/root;
on Windows the GUI app auto-elevates, mac/Linux prompt via the OS when needed.

## Auto-update (the app updates itself)

PatchPilot updates itself across all your machines via the Tauri updater + GitHub Releases.

- On launch it silently checks `github.com/bullsim/patchpilot` releases; the header pill shows
  **✓ Up to date** or **⬇ Update to vX** (click to install + relaunch). It also re-checks on demand.
- Updates are cryptographically signed; clients only accept builds signed with the private key.
  Public key is baked into `tauri.conf.json`; the **private key lives outside the repo** at
  `~/.tauri/patchpilot.key` — never commit it.

### Cutting a release

1. Bump `version` in `src-tauri/tauri.conf.json` (and `package.json`).
2. One-time GitHub setup: create a **public** repo `bullsim/patchpilot`, push this folder, and add
   repo secrets:
   - `TAURI_SIGNING_PRIVATE_KEY` = contents of `~/.tauri/patchpilot.key`
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` = (empty unless you set one)
3. Tag and push:
   ```bash
   git tag v0.2.0 && git push origin v0.2.0
   ```
   The [release workflow](.github/workflows/release.yml) builds, signs, and publishes the installer
   plus `latest.json`. Every running app picks it up on next launch.

### Building a signed release locally

```bash
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/patchpilot.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
npm run tauri build
```

## NVIDIA GPU drivers

winget only updates the **NVIDIA App** shell — the GPU driver in its *Drivers* tab has no
silent CLI. PatchPilot installs the actual driver via
[TinyNvidiaUpdateChecker](https://github.com/ZenitH-AT/TinyNvidiaUpdateChecker) if it's
installed:

```powershell
winget install ZenitH-AT.TinyNvidiaUpdateChecker
```

Run it once to choose **Studio** vs **Game Ready** (it remembers the choice); after that the
Nvidia card auto-installs the latest driver headlessly. If TNUC isn't installed, the card says
so and the driver stays a manual click in the NVIDIA App.

## Notes

- Updaters are added to `registry.rs` (metadata + `applies`) and `updaters.rs` (the work).
  macOS/Linux updaters slot in behind the same `Component` model — the UI never changes.
- `--silent` writes a transcript to `%LOCALAPPDATA%\PatchPilot\logs\`.
