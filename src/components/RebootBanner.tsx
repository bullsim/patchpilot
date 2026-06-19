import { useEffect, useState } from "react";

interface Props {
  /** seconds remaining before auto-02:00, or null to hide countdown */
  countdown: number | null;
  scheduledText: string | null;
  onNow: () => void;
  onTonight: () => void;
  onCustom: () => void;
  onCancel: () => void;
}

export function RebootBanner({
  countdown,
  scheduledText,
  onNow,
  onTonight,
  onCustom,
  onCancel,
}: Props) {
  const [, force] = useState(0);
  // re-render once a second so the countdown ticks
  useEffect(() => {
    if (countdown === null) return;
    const t = setInterval(() => force((n) => n + 1), 1000);
    return () => clearInterval(t);
  }, [countdown]);

  const sub =
    scheduledText ??
    (countdown !== null
      ? `Choose a restart option (auto 02:00 in ${fmt(countdown)})`
      : "");

  return (
    <div className="reboot">
      <div className="reboot-left">
        <span className="reboot-warn">⚠</span>
        <div>
          <div className="reboot-title">
            Firmware updates require a restart
          </div>
          <div className="reboot-sub">{sub}</div>
        </div>
      </div>
      {!scheduledText && (
        <div className="reboot-actions">
          <button type="button" className="rbtn danger" onClick={onNow}>
            Restart Now
          </button>
          <button type="button" className="rbtn warn" onClick={onTonight}>
            Tonight 02:00
          </button>
          <button type="button" className="rbtn" onClick={onCustom}>
            Custom…
          </button>
          <button type="button" className="rbtn" onClick={onCancel}>
            ✕ Cancel
          </button>
        </div>
      )}
    </div>
  );
}

function fmt(totalSecs: number): string {
  const s = Math.max(0, totalSecs);
  const m = Math.floor(s / 60);
  const r = s % 60;
  return `${String(m).padStart(2, "0")}:${String(r).padStart(2, "0")}`;
}
