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

export interface AppConfig {
  scheduledRunMode: RunMode;
  teamsWebhook: string;
  reportUrl: string;
  haUrl: string;
  haToken: string;
  components: Record<string, boolean>;
}
