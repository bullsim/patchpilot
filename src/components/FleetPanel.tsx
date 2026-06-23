import { useEffect, useState } from "react";
import * as api from "../lib/api";
import type { FleetMachine } from "../lib/types";

export function FleetPanel({ onClose }: { onClose: () => void }) {
  const [rows, setRows] = useState<FleetMachine[] | null>(null);
  const [err, setErr] = useState<string | null>(null);

  const load = () => {
    setErr(null);
    api
      .getFleet()
      .then((r) => setRows(r.sort((a, b) => a.hostname.localeCompare(b.hostname))))
      .catch((e) => setErr(String(e)));
  };
  useEffect(load, []);

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal modal-wide" onClick={(e) => e.stopPropagation()}>
        <h2>Fleet</h2>
        {err && <div className="field-label">{err}</div>}
        {!err && rows === null && <div className="field-label">Loading…</div>}
        {!err && rows?.length === 0 && (
          <div className="field-label">No machines have reported yet.</div>
        )}
        <div className="grid">
          {rows?.map((m) => {
            const c = m.fail > 0 ? "var(--red)" : m.warn > 0 ? "var(--amber)" : "var(--green)";
            return (
              <div className="ucard" key={m.hostname} style={{ ["--c" as string]: c }}>
                <div className="ucard-head">
                  <span className="ucard-emoji">🖥️</span>
                  {m.rebootRequired && (
                    <span className="ucard-status" style={{ ["--c" as string]: "var(--amber)" }}>
                      reboot
                    </span>
                  )}
                </div>
                <div className="ucard-name">{m.hostname}</div>
                <div className="ucard-detail">
                  {[m.manufacturer, m.model].filter(Boolean).join(" ")} · {m.os} · v{m.version}
                </div>
                <div className="fleet-counts">
                  <span style={{ color: "var(--green)" }}>✓{m.ok}</span>
                  <span style={{ color: "var(--amber)" }}>⚠{m.warn}</span>
                  <span style={{ color: "var(--red)" }}>✗{m.fail}</span>
                  <span className="fleet-when">{ago(m.timestamp)}</span>
                </div>
              </div>
            );
          })}
        </div>
        <div className="modal-actions">
          <button type="button" className="mode-btn" onClick={load}>
            Refresh
          </button>
          <button type="button" className="mode-btn primary" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}

function ago(iso: string): string {
  try {
    const mins = (Date.now() - new Date(iso).getTime()) / 60000;
    if (mins < 60) return `${Math.round(mins)}m ago`;
    if (mins < 1440) return `${Math.round(mins / 60)}h ago`;
    return `${Math.round(mins / 1440)}d ago`;
  } catch {
    return "";
  }
}
