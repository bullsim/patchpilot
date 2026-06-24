import { useCallback, useEffect, useState } from "react";
import * as api from "../lib/api";
import type { FleetMachine } from "../lib/types";

export function FleetPanel({
  onClose,
  complianceDays,
}: {
  onClose: () => void;
  complianceDays: number;
}) {
  const [rows, setRows] = useState<FleetMachine[] | null>(null);
  const [err, setErr] = useState<string | null>(null);

  const load = useCallback(() => {
    setErr(null);
    api
      .getFleet()
      .then((r) => setRows(r.sort((a, b) => a.hostname.localeCompare(b.hostname))))
      .catch((e) => setErr(String(e)));
  }, []);

  useEffect(() => {
    load();
    const t = setInterval(load, 30000); // live refresh while open
    return () => clearInterval(t);
  }, [load]);

  const windowDays = complianceDays || 7;
  const judged = (rows ?? []).map((m) => ({ m, ...compliance(m, windowDays) }));
  const okCount = judged.filter((j) => j.compliant).length;

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal modal-wide" onClick={(e) => e.stopPropagation()}>
        <div className="fleet-head">
          <h2>Fleet</h2>
          {rows && rows.length > 0 && (
            <span className={`fleet-summary ${okCount === rows.length ? "ok" : "bad"}`}>
              {okCount}/{rows.length} compliant
            </span>
          )}
        </div>

        {err && <div className="field-label">{err}</div>}
        {!err && rows === null && <div className="field-label">Loading…</div>}
        {!err && rows?.length === 0 && (
          <div className="field-label">No machines have reported yet.</div>
        )}

        <div className="grid">
          {judged.map(({ m, compliant, reasons }) => {
            const c = compliant ? "var(--green)" : "var(--red)";
            return (
              <div className="ucard" key={m.hostname} style={{ ["--c" as string]: c }}>
                <div className="ucard-head">
                  <span className="ucard-emoji">🖥️</span>
                  <span className="ucard-status">{compliant ? "✓ OK" : "✗ Check"}</span>
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
                {!compliant && <div className="fleet-reason">{reasons.join(" · ")}</div>}
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

function compliance(m: FleetMachine, days: number): { compliant: boolean; reasons: string[] } {
  const reasons: string[] = [];
  const ageDays = (Date.now() - new Date(m.timestamp).getTime()) / 86400000;
  if (ageDays > days) reasons.push(`stale (${Math.round(ageDays)}d)`);
  if (m.fail > 0) reasons.push(`${m.fail} failed`);
  if (m.rebootRequired) reasons.push("reboot pending");
  return { compliant: reasons.length === 0, reasons };
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
