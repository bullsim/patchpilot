import type { ComponentStatus, Status } from "../lib/types";

const COLOR: Record<Status, string> = {
  Pending: "var(--slate)",
  Running: "var(--blue)",
  Success: "var(--green)",
  Warning: "var(--amber)",
  Failed: "var(--red)",
  Skipped: "var(--slate)",
};

const ICON: Record<string, string> = {
  "windows-update": "🪟",
  winget: "📦",
  office: "📊",
  antigravity: "🌐",
  dell: "💻",
  surface: "🖥️",
  nvidia: "🎮",
  intel: "🧩",
  razer: "🐍",
  logitech: "🖱️",
  crucial: "💾",
};

export function Card({ s }: { s: ComponentStatus }) {
  const color = COLOR[s.status] ?? COLOR.Pending;
  const showBar = s.progress >= 0 && s.status === "Running";
  return (
    <div className="ucard" style={{ ["--c" as string]: color }}>
      <div className="ucard-head">
        <span className="ucard-emoji">{ICON[s.id] ?? "⚙️"}</span>
        <span className="ucard-status">
          {s.status === "Running" && <span className="spin" />}
          {s.status}
        </span>
      </div>
      <div className="ucard-name">{s.name}</div>
      <div className="ucard-detail" title={s.detail}>
        {s.detail || "—"}
      </div>
      {showBar && (
        <div className="ucard-bar">
          <div
            className="ucard-bar-fill"
            style={{ width: `${Math.max(4, Math.min(100, s.progress))}%` }}
          />
        </div>
      )}
    </div>
  );
}
