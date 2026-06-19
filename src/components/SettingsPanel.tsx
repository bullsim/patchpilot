import { useState } from "react";
import type { AppConfig, RunMode } from "../lib/types";

interface Props {
  config: AppConfig;
  onSave: (c: AppConfig) => void;
  onClose: () => void;
}

export function SettingsPanel({ config, onSave, onClose }: Props) {
  const [draft, setDraft] = useState<AppConfig>(structuredClone(config));

  const toggle = (name: string) =>
    setDraft((d) => ({
      ...d,
      components: { ...d.components, [name]: !d.components[name] },
    }));

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h2>Settings</h2>

        <label className="field">
          <span>Scheduled run mode</span>
          <select
            value={draft.scheduledRunMode}
            onChange={(e) =>
              setDraft((d) => ({ ...d, scheduledRunMode: e.target.value as RunMode }))
            }
          >
            <option value="All">All</option>
            <option value="Software">Software</option>
            <option value="Firmware">Firmware</option>
          </select>
        </label>

        <label className="field">
          <span>Teams webhook (optional)</span>
          <input
            type="text"
            value={draft.teamsWebhook}
            placeholder="https://…"
            onChange={(e) => setDraft((d) => ({ ...d, teamsWebhook: e.target.value }))}
          />
        </label>

        <label className="field">
          <span>Fleet dashboard URL (blank = off)</span>
          <input
            type="text"
            value={draft.reportUrl}
            placeholder="https://patchpilot.bullers.com/api/report"
            onChange={(e) => setDraft((d) => ({ ...d, reportUrl: e.target.value }))}
          />
        </label>

        <div className="field-label">Components</div>
        <div className="toggles">
          {Object.keys(draft.components)
            .sort()
            .map((name) => (
              <label key={name} className="toggle">
                <input
                  type="checkbox"
                  checked={draft.components[name]}
                  onChange={() => toggle(name)}
                />
                <span>{name}</span>
              </label>
            ))}
        </div>

        <div className="modal-actions">
          <button type="button" className="mode-btn" onClick={onClose}>
            Cancel
          </button>
          <button type="button" className="mode-btn primary" onClick={() => onSave(draft)}>
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
