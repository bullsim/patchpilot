import { useCallback, useEffect, useState } from "react";
import * as api from "../lib/api";
import type { FleetMachine } from "../lib/types";
import { compliance, ago } from "../lib/fleet";

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

  const cmd = (host: string, action: string, confirmMsg: string) => {
    if (!confirm(confirmMsg + "\n\nThe machine runs it within ~5 minutes (next check-in).")) return;
    api
      .sendCommand(host, action)
      .then(() => api.notify("PatchPilot", `Command queued for ${host}`))
      .catch((e) => alert(String(e)));
  };

  const problems = (m: FleetMachine): string[] =>
    (m.components ?? [])
      .filter((c) => c.status === "Failed" || c.status === "Warning")
      .map((c) => `${c.status === "Failed" ? "✗" : "⚠"} ${c.name}`);

  const broadcast = (action: string, confirmMsg: string) => {
    if (!rows || !confirm(confirmMsg + "\n\nEach runs it within ~5 minutes.")) return;
    Promise.allSettled(rows.map((m) => api.sendCommand(m.hostname, action)))
      .then(() => api.notify("PatchPilot", `Command queued for ${rows.length} machine(s)`))
      .catch(() => {});
  };

  const windowDays = complianceDays || 7;
  const judged = (rows ?? []).map((m) => ({ m, ...compliance(m, windowDays) }));
  const okCount = judged.filter((j) => j.compliant).length;

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal modal-wide" onClick={(e) => e.stopPropagation()}>
        <div className="fleet-head">
          <h2>Fleet</h2>
          <div className="fleet-head-right">
            {rows && rows.length > 0 && (
              <span className={`fleet-summary ${okCount === rows.length ? "ok" : "bad"}`}>
                {okCount}/{rows.length} compliant
              </span>
            )}
            {rows && rows.length > 0 && (
              <button
                type="button"
                className="fleet-btn"
                onClick={() => broadcast("run-all", `Run all updates on ALL ${rows.length} machines?`)}
              >
                ▶ Run all machines
              </button>
            )}
          </div>
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
                {problems(m).length > 0 && (
                  <div className="fleet-issues" title="Components needing attention">
                    {problems(m).join(", ")}
                  </div>
                )}
                <div className="fleet-actions">
                  <button
                    type="button"
                    className="fleet-btn"
                    title="Run all updates on this machine"
                    onClick={() => cmd(m.hostname, "run-all", `Run all updates on ${m.hostname}?`)}
                  >
                    ▶ Run
                  </button>
                  <button
                    type="button"
                    className="fleet-btn"
                    title="Restart this machine"
                    onClick={() => cmd(m.hostname, "reboot", `Restart ${m.hostname}?`)}
                  >
                    ⟳ Reboot
                  </button>
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

