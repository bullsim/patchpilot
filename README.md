# PatchPilot

A slick cross-platform desktop app that keeps your machines up to date. It is the
successor to `daily_updates_v5.3.ps1` — the same proven update engine, rebuilt as a real
app with a live UI and a path to macOS and Linux.

- **Windows (now):** Windows Update, Microsoft Store, winget, Office C2R, Dell Command
  Update, Surface, Nvidia, Intel, Razer, Logitech, Crucial.

> **Principle: update what's installed, never install new software.** PatchPilot only
> upgrades apps/firmware tools that are already present on the machine. Components that
> aren't installed (e.g. Crucial Storage Executive, Intel DSA, Dell Command Update) are
> skipped, not installed. `winget upgrade --all` likewise only updates existing packages.
- **macOS (planned):** `softwareupdate`, `brew`, `mas`.
- **Linux/Zorin (planned):** `apt`, `flatpak`, `snap`, `fwupdmgr`.

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
  config.rs                load/save config.json (+ the 11 component names)
  registry.rs              ComponentMeta + applies() + mode/config selection
  orchestrator.rs          Reporter trait, sequencing, counts, summary, reboot flag
  updaters.rs              the 11 Windows updaters (ported from v5.3)
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
# -> src-tauri/target/release/patchpilot.exe
# -> src-tauri/target/release/bundle/msi/PatchPilot_x.y.z_x64_en-US.msi
```

## Scheduled (headless) runs

The app can run with no window for Task Scheduler:

```bash
patchpilot.exe --silent --mode All        # or Software / Firmware
```

Register a daily task with the helper (run from an elevated PowerShell):

```powershell
.\install-scheduled-task.ps1 -Time 03:00 -Mode All
```

Updates require administrator rights; run the app (or the scheduled task) elevated.

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

## Notes

- Updaters are added to `registry.rs` (metadata + `applies`) and `updaters.rs` (the work).
  macOS/Linux updaters slot in behind the same `Component` model — the UI never changes.
- `--silent` writes a transcript to `%LOCALAPPDATA%\PatchPilot\logs\`.
