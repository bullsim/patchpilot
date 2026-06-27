import type { FleetMachine } from "./types";

export interface Compliance {
  compliant: boolean;
  reasons: string[];
}

/** A machine is compliant if it ran successfully within `days` and has no failures/reboot. */
export function compliance(m: FleetMachine, days: number): Compliance {
  const reasons: string[] = [];
  const ageDays = (Date.now() - new Date(m.timestamp).getTime()) / 86400000;
  if (ageDays > days) reasons.push(`stale (${Math.round(ageDays)}d)`);
  if (m.fail > 0) reasons.push(`${m.fail} failed`);
  if (m.rebootRequired) reasons.push("reboot pending");
  return { compliant: reasons.length === 0, reasons };
}

/** Compare dotted version strings. >0 if a newer than b, <0 if older, 0 if equal. */
export function cmpVersion(a: string, b: string): number {
  const pa = (a || "").split(".").map((n) => parseInt(n, 10) || 0);
  const pb = (b || "").split(".").map((n) => parseInt(n, 10) || 0);
  const len = Math.max(pa.length, pb.length);
  for (let i = 0; i < len; i++) {
    const d = (pa[i] || 0) - (pb[i] || 0);
    if (d !== 0) return d;
  }
  return 0;
}

/** The highest PatchPilot version seen across the fleet (the upgrade target). */
export function newestVersion(rows: FleetMachine[]): string {
  return rows.reduce((max, m) => (cmpVersion(m.version, max) > 0 ? m.version : max), "0.0.0");
}

export function ago(iso: string): string {
  try {
    const mins = (Date.now() - new Date(iso).getTime()) / 60000;
    if (mins < 60) return `${Math.round(mins)}m ago`;
    if (mins < 1440) return `${Math.round(mins / 60)}h ago`;
    return `${Math.round(mins / 1440)}d ago`;
  } catch {
    return "";
  }
}
