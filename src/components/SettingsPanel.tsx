import { useState } from "react";
import * as api from "../lib/api";
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

        <label className="toggle toggle-spaced">
          <input
            type="checkbox"
            checked={draft.scheduleEnabled}
            onChange={(e) => setDraft((d) => ({ ...d, scheduleEnabled: e.target.checked }))}
          />
          <span>Run automatically every day</span>
        </label>

        <div className="field-row">
          <label className="field">
            <span>Time</span>
            <input
              type="time"
              value={draft.scheduleTime}
              disabled={!draft.scheduleEnabled}
              onChange={(e) => setDraft((d) => ({ ...d, scheduleTime: e.target.value }))}
            />
          </label>
          <label className="field">
            <span>Run mode</span>
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
        </div>

        <label className="toggle toggle-spaced">
          <input
            type="checkbox"
            checked={draft.autoReboot}
            onChange={(e) => setDraft((d) => ({ ...d, autoReboot: e.target.checked }))}
          />
          <span>Auto-restart after unattended firmware updates</span>
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
          <span>Home Assistant URL (blank = off)</span>
          <input
            type="text"
            value={draft.haUrl}
            placeholder="http://homeassistant.local:8123"
            onChange={(e) => setDraft((d) => ({ ...d, haUrl: e.target.value }))}
          />
        </label>

        <label className="field">
          <span>Home Assistant token (long-lived access token)</span>
          <input
            type="password"
            value={draft.haToken}
            placeholder="paste from HA → Profile → Long-lived tokens"
            onChange={(e) => setDraft((d) => ({ ...d, haToken: e.target.value }))}
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

        <div className="field-label">Sync across machines</div>
        <div className="field-row">
          <button
            type="button"
            className="mode-btn"
            onClick={async () => {
              const path = await api.exportConfig().catch(() => null);
              alert(path ? `Exported to:\n${path}\n\nCopy that file to another machine's home folder, then Import there.` : "Export failed");
            }}
          >
            ⬆ Export config
          </button>
          <button
            type="button"
            className="mode-btn"
            onClick={async () => {
              try {
                const imported = await api.importConfig();
                onSave(imported);
              } catch (e) {
                alert(String(e));
              }
            }}
          >
            ⬇ Import config
          </button>
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
