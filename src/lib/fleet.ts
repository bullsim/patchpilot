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
