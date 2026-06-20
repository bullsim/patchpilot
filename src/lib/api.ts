import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import type {
  AppConfig,
  ComponentStatus,
  RunMode,
  RunSummary,
  SystemInfo,
} from "./types";

// ---- Commands (Rust) ----

export const getSystemInfo = () => invoke<SystemInfo>("get_system_info");
export const getConfig = () => invoke<AppConfig>("get_config");
export const saveConfig = (config: AppConfig) =>
  invoke<void>("save_config", { config });

/** List the components that apply to this machine for the given mode (for initial cards). */
export const planRun = (mode: RunMode) =>
  invoke<ComponentStatus[]>("plan_run", { mode });

export const startRun = (mode: RunMode) => invoke<void>("start_run", { mode });
/** Run a single component by id (clicking a card). */
export const startRunOne = (id: string) => invoke<void>("run_one", { id });
export const cancelRun = () => invoke<void>("cancel_run");
export const openLatestLog = () => invoke<void>("open_latest_log");

/** when: ISO 8601 local datetime, or "now". */
export const scheduleReboot = (when: string) =>
  invoke<string>("schedule_reboot", { when });
export const cancelReboot = () => invoke<void>("cancel_reboot");
export const getPendingReboot = () => invoke<string | null>("get_pending_reboot");

/** True if PatchPilot is running with admin/root (always true on mac/linux). */
export const getIsAdmin = () => invoke<boolean>("is_admin");
/** Relaunch elevated (Windows UAC). The current process exits if accepted. */
export const restartElevated = () => invoke<void>("relaunch_elevated");

// ---- Events (Rust -> UI) ----

export const onComponentStatus = (cb: (s: ComponentStatus) => void): Promise<UnlistenFn> =>
  listen<ComponentStatus>("component-status", (e) => cb(e.payload));

export const onRunFinished = (cb: (s: RunSummary) => void): Promise<UnlistenFn> =>
  listen<RunSummary>("run-finished", (e) => cb(e.payload));

export const onLog = (cb: (line: string) => void): Promise<UnlistenFn> =>
  listen<string>("log-line", (e) => cb(e.payload));

// ---- App self-update (Tauri updater) ----

export type { Update };

/** Returns an Update handle if a newer version is published, else null. */
export const checkForAppUpdate = (): Promise<Update | null> => check();

export const relaunchApp = (): Promise<void> => relaunch();
