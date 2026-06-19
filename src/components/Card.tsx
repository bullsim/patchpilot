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
  homeassistant: "🏠",
  // macOS
  "macos-update": "🍎",
  brew: "🍺",
  mas: "🛍️",
  // Linux
  apt: "📦",
  flatpak: "📦",
  snap: "📦",
  fwupd: "🔌",
};

export function Card({
  s,
  onClick,
  disabled,
}: {
  s: ComponentStatus;
  onClick?: () => void;
  disabled?: boolean;
}) {
  const color = COLOR[s.status] ?? COLOR.Pending;
  const showBar = s.progress >= 0 && s.status === "Running";
  const clickable = !!onClick && !disabled;
  return (
    <div
      className={`ucard${clickable ? " clickable" : ""}`}
      style={{ ["--c" as string]: color }}
      onClick={clickable ? onClick : undefined}
      title={clickable ? `Update ${s.name} now` : undefined}
    >
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
