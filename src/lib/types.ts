// Shared types — mirror the Rust structs (serde camelCase).

export type Status =
  | "Pending"
  | "Running"
  | "Success"
  | "Warning"
  | "Failed"
  | "Skipped";

export type Category = "Software" | "Firmware";

export type RunMode = "All" | "Software" | "Firmware";

export interface ComponentStatus {
  id: string;
  name: string;
  category: Category;
  status: Status;
  detail: string;
  /** 0-100, or -1 when indeterminate. */
  progress: number;
}

export interface SystemInfo {
  manufacturer: string;
  model: string;
  gpus: string;
  os: string;
  isDell: boolean;
  isSurface: boolean;
  hasNvidia: boolean;
  hasIntelGpu: boolean;
  appRazer: boolean;
  appLogitech: boolean;
}

export interface RunSummary {
  mode: RunMode;
  ok: number;
  warn: number;
  fail: number;
  skip: number;
  durationSecs: number;
  rebootRequired: boolean;
}

export interface HistoryEntry {
  timestamp: string;
  hostname: string;
  mode: RunMode;
  ok: number;
  warn: number;
  fail: number;
  skip: number;
  durationSecs: number;
  rebootRequired: boolean;
}

export interface AppConfig {
  scheduledRunMode: RunMode;
  teamsWebhook: string;
  reportUrl: string;
  scheduleEnabled: boolean;
  scheduleTime: string;
  autoReboot: boolean;
  haUrl: string;
  haToken: string;
  wingetExcludes: string[];
  fleetGist: string;
  fleetToken: string;
  complianceDays: number;
  components: Record<string, boolean>;
}

export interface FleetMachine {
  hostname: string;
  os: string;
  manufacturer?: string;
  model?: string;
  version: string;
  mode: RunMode;
  ok: number;
  warn: number;
  fail: number;
  skip: number;
  rebootRequired: boolean;
  durationSecs: number;
  timestamp: string;
}
