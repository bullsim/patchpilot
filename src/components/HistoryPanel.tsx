import { useEffect, useState } from "react";
import * as api from "../lib/api";
import type { HistoryEntry } from "../lib/types";

export function HistoryPanel({ onClose }: { onClose: () => void }) {
  const [rows, setRows] = useState<HistoryEntry[] | null>(null);

  useEffect(() => {
    api.getHistory().then(setRows).catch(() => setRows([]));
  }, []);

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>Run history</h2>
        {rows === null && <div className="field-label">Loading…</div>}
        {rows?.length === 0 && <div className="field-label">No runs recorded yet.</div>}
        <div className="history-list">
          {rows?.map((r, i) => (
            <div className="history-row" key={i}>
              <div className="history-when">
                <div>{fmt(r.timestamp)}</div>
                <div className="history-sub">
                  {label(r.mode)} · {fmtDur(r.durationSecs)}
                  {r.rebootRequired ? " · ⟳ reboot" : ""}
                </div>
              </div>
              <div className="history-counts">
                <span style={{ color: "#22c55e" }}>✓{r.ok}</span>
                <span style={{ color: "#f59e0b" }}>⚠{r.warn}</span>
                <span style={{ color: "#ef4444" }}>✗{r.fail}</span>
              </div>
            </div>
          ))}
        </div>
        <div className="modal-actions">
          <button type="button" className="mode-btn" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}

function label(m: string): string {
  return m === "All" ? "All" : m === "Software" ? "Software" : "Firmware";
}
function fmtDur(s: number): string {
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
}
function fmt(iso: string): string {
  try {
    return new Date(iso).toLocaleString(undefined, {
      month: "short",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return iso;
  }
}
