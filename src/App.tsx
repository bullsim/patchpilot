import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Card } from "./components/Card";
import { ProgressRing } from "./components/ProgressRing";
import { RebootBanner } from "./components/RebootBanner";
import { SettingsPanel } from "./components/SettingsPanel";
import * as api from "./lib/api";
import type {
  AppConfig,
  ComponentStatus,
  RunMode,
  RunSummary,
  SystemInfo,
} from "./lib/types";

const REBOOT_DECISION_SECS = 5 * 60;

export default function App() {
  const [sys, setSys] = useState<SystemInfo | null>(null);
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [cards, setCards] = useState<Map<string, ComponentStatus>>(new Map());
  const [running, setRunning] = useState(false);
  const [mode, setMode] = useState<RunMode>("All");
  const [summary, setSummary] = useState<RunSummary | null>(null);
  const [multi, setMulti] = useState(false); // true = Run-All/mode run; false = single card
  const [startedAt, setStartedAt] = useState<number | null>(null);
  const [elapsed, setElapsed] = useState(0);
  const [showSettings, setShowSettings] = useState(false);

  const [rebootCountdown, setRebootCountdown] = useState<number | null>(null);
  const [rebootScheduled, setRebootScheduled] = useState<string | null>(null);
  const rebootDeadline = useRef<number | null>(null);

  // app self-update
  type UpdState = "idle" | "checking" | "current" | "available" | "downloading" | "error";
  const [upd, setUpd] = useState<UpdState>("idle");
  const [updVer, setUpdVer] = useState<string>("");
  const updHandle = useRef<api.Update | null>(null);

  const checkAppUpdate = useCallback(async (manual: boolean) => {
    setUpd("checking");
    try {
      const u = await api.checkForAppUpdate();
      if (u) {
        updHandle.current = u;
        setUpdVer(u.version);
        setUpd("available");
      } else {
        setUpd("current");
        if (manual) alert("PatchPilot is up to date.");
      }
    } catch (e) {
      console.error(e);
      setUpd("error");
      if (manual) alert("Could not check for updates (no published release yet?).");
    }
  }, []);

  const installAppUpdate = useCallback(async () => {
    if (!updHandle.current) return checkAppUpdate(true);
    if (!confirm(`Install PatchPilot ${updVer} now? The app will restart.`)) return;
    setUpd("downloading");
    try {
      await updHandle.current.downloadAndInstall();
      await api.relaunchApp();
    } catch (e) {
      console.error(e);
      setUpd("error");
      alert("Update failed to install.");
    }
  }, [updVer, checkAppUpdate]);

  // check once on launch (silent)
  useEffect(() => {
    checkAppUpdate(false);
  }, [checkAppUpdate]);

  useEffect(() => {
    api.getSystemInfo().then(setSys).catch(console.error);
    api.getConfig().then(setConfig).catch(console.error);
    // Show idle component cards immediately so any can be clicked to run on its own.
    api
      .planRun("All")
      .then((p) => setCards(new Map(p.map((c) => [c.id, c]))))
      .catch(() => {});
    api.getPendingReboot().then((iso) => {
      if (iso) setRebootScheduled(`Restart scheduled for ${fmtWhen(iso)}`);
    });
  }, []);

  useEffect(() => {
    const unlistens: Array<Promise<() => void>> = [];
    unlistens.push(
      api.onComponentStatus((s) =>
        setCards((prev) => {
          const next = new Map(prev);
          next.set(s.id, s);
          return next;
        })
      )
    );
    unlistens.push(
      api.onRunFinished((s) => {
        setSummary(s);
        setRunning(false);
        if (s.rebootRequired && !rebootScheduled) {
          rebootDeadline.current = Date.now() + REBOOT_DECISION_SECS * 1000;
          setRebootCountdown(REBOOT_DECISION_SECS);
        }
      })
    );
    return () => {
      unlistens.forEach((p) => p.then((u) => u()));
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rebootScheduled]);

  useEffect(() => {
    if (!running || startedAt === null) return;
    const t = setInterval(() => setElapsed(Math.floor((Date.now() - startedAt) / 1000)), 1000);
    return () => clearInterval(t);
  }, [running, startedAt]);

  useEffect(() => {
    if (rebootCountdown === null || rebootDeadline.current === null) return;
    const t = setInterval(async () => {
      const remaining = Math.ceil((rebootDeadline.current! - Date.now()) / 1000);
      if (remaining <= 0) {
        clearInterval(t);
        const when = await api.scheduleReboot("02:00").catch(() => null);
        setRebootCountdown(null);
        if (when) setRebootScheduled(`Auto-scheduled restart for ${fmtWhen(when)}`);
      } else {
        setRebootCountdown(remaining);
      }
    }, 1000);
    return () => clearInterval(t);
  }, [rebootCountdown]);

  const run = useCallback(async (m: RunMode) => {
    setMode(m);
    setMulti(true);
    setSummary(null);
    setRebootCountdown(null);
    rebootDeadline.current = null;
    const plan = await api.planRun(m).catch(() => [] as ComponentStatus[]);
    setCards(new Map(plan.map((c) => [c.id, c])));
    setStartedAt(Date.now());
    setElapsed(0);
    setRunning(true);
    await api.startRun(m).catch((e) => {
      console.error(e);
      setRunning(false);
    });
  }, []);

  // Click one card → update just that component.
  const runOne = useCallback(
    async (id: string) => {
      if (running) return;
      setMulti(false);
      setSummary(null);
      setCards((prev) => {
        const n = new Map(prev);
        const c = n.get(id);
        if (c) n.set(id, { ...c, status: "Running", detail: "Starting…", progress: 0 });
        return n;
      });
      setRunning(true);
      await api.startRunOne(id).catch((e) => {
        console.error(e);
        setRunning(false);
      });
    },
    [running]
  );

  const sorted = useMemo(
    () => [...cards.values()].sort((a, b) => a.name.localeCompare(b.name)),
    [cards]
  );
  const counts = useMemo(() => tally(sorted), [sorted]);
  const overall = useMemo(() => {
    if (sorted.length === 0) return 0;
    const done = sorted.filter((c) =>
      ["Success", "Warning", "Failed", "Skipped"].includes(c.status)
    ).length;
    return Math.round((done / sorted.length) * 100);
  }, [sorted]);

  const stateColor = useMemo(() => {
    if (running) return "var(--blue)";
    if (counts.fail > 0) return "var(--red)";
    if (counts.warn > 0) return "var(--amber)";
    if (counts.ok > 0) return "var(--green)";
    return "var(--blue)";
  }, [running, counts]);

  const handleNow = async () => {
    if (!confirm("Restart now? Unsaved work will be lost. Windows restarts in 60s.")) return;
    await api.scheduleReboot("now");
    setRebootCountdown(null);
    setRebootScheduled("Restarting in 60 seconds…");
  };
  const handleTonight = async () => {
    const when = await api.scheduleReboot("02:00");
    setRebootCountdown(null);
    setRebootScheduled(`Restart scheduled for ${fmtWhen(when)}`);
  };
  const handleCustom = async () => {
    const t = prompt("Restart time (24h, e.g. 03:30):", "02:00");
    if (!t) return;
    const when = await api.scheduleReboot(t.trim()).catch(() => null);
    if (when) {
      setRebootCountdown(null);
      setRebootScheduled(`Restart scheduled for ${fmtWhen(when)}`);
    } else {
      alert("Invalid time. Use HH:mm (e.g. 03:30).");
    }
  };
  const handleCancelReboot = async () => {
    await api.cancelReboot();
    setRebootCountdown(null);
    setRebootScheduled(null);
  };

  const showReboot = rebootCountdown !== null || rebootScheduled !== null;
  const hwChips = sys ? chipList(sys) : [];

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">
          <div className="logo">P</div>
          <div>
            <div className="brand-name">PatchPilot</div>
            <div className="brand-sub">
              {sys ? `${tidy(sys.manufacturer)} ${sys.model} · ${shortOs(sys.os)}` : "Detecting hardware…"}
            </div>
          </div>
        </div>
        <div className="topbar-right">
          <div className="chips">
            {hwChips.map((c) => (
              <span key={c} className="chip">
                {c}
              </span>
            ))}
          </div>
          <UpdatePill state={upd} version={updVer} onClick={upd === "available" ? installAppUpdate : () => checkAppUpdate(true)} />
          <button type="button" className="icon-btn" title="Open latest log" onClick={() => api.openLatestLog()}>
            📄
          </button>
          <button type="button" className="icon-btn" title="Settings" onClick={() => setShowSettings(true)}>
            ⚙️
          </button>
        </div>
      </header>

      <div className="deck">
        <div className="run-modes">
          <button type="button" className="mode-btn primary" disabled={running} onClick={() => run("All")}>
            <span className="mode-emoji">▶</span> Run All
          </button>
          <button type="button" className="mode-btn" disabled={running} onClick={() => run("Software")}>
            <span className="mode-emoji">🔧</span> Software
          </button>
          <button type="button" className="mode-btn" disabled={running} onClick={() => run("Firmware")}>
            <span className="mode-emoji">💾</span> Firmware
          </button>
        </div>
        <div className="deck-spacer" />
        {running && (
          <button type="button" className="stop-btn" onClick={() => api.cancelRun()}>
            ✕ Stop
          </button>
        )}
      </div>

      {(running || summary) && multi && (
        <section className="summary">
          <ProgressRing pct={overall} color={stateColor} />
          <div className="summary-info">
            <div className="summary-title">
              {running ? "Updating your machine…" : "Run complete"}
            </div>
            <div className="summary-sub">
              {modeLabel(mode)} · {fmtDur(running ? elapsed : summary?.durationSecs ?? elapsed)}
            </div>
            <div className="summary-counts">
              <Count color="var(--green)" n={counts.ok} label="ok" />
              <Count color="var(--amber)" n={counts.warn} label="warnings" />
              <Count color="var(--red)" n={counts.fail} label="failed" />
              <Count color="var(--slate)" n={counts.skip} label="skipped" />
            </div>
          </div>
        </section>
      )}

      <main className="grid">
        {sorted.map((c) => (
          <Card key={c.id} s={c} onClick={() => runOne(c.id)} disabled={running} />
        ))}
        {sorted.length === 0 && (
          <div className="empty">
            <div>
              <div className="empty-big">🚀</div>
              Detecting components… or pick a run mode above.
            </div>
          </div>
        )}
      </main>

      {showReboot && (
        <RebootBanner
          countdown={rebootCountdown}
          scheduledText={rebootScheduled}
          onNow={handleNow}
          onTonight={handleTonight}
          onCustom={handleCustom}
          onCancel={handleCancelReboot}
        />
      )}

      <footer className="statusbar">
        <span>{running ? "Running… live status updates below" : "Ready"}</span>
        <span>{sys ? sys.gpus : ""}</span>
      </footer>

      {showSettings && config && (
        <SettingsPanel
          config={config}
          onSave={async (c) => {
            await api.saveConfig(c);
            setConfig(c);
            setShowSettings(false);
            // Refresh cards so newly-configured components (e.g. Home Assistant) appear.
            if (!running) {
              api
                .planRun("All")
                .then((p) => setCards(new Map(p.map((x) => [x.id, x]))))
                .catch(() => {});
            }
          }}
          onClose={() => setShowSettings(false)}
        />
      )}
    </div>
  );
}

function UpdatePill({
  state,
  version,
  onClick,
}: {
  state: "idle" | "checking" | "current" | "available" | "downloading" | "error";
  version: string;
  onClick: () => void;
}) {
  const map = {
    idle: { text: "Check for updates", cls: "" },
    checking: { text: "Checking…", cls: "" },
    current: { text: "✓ Up to date", cls: "ok" },
    available: { text: `⬇ Update to ${version}`, cls: "avail" },
    downloading: { text: "Updating…", cls: "avail" },
    error: { text: "Check for updates", cls: "" },
  } as const;
  const m = map[state];
  return (
    <button
      type="button"
      className={`update-pill ${m.cls}`}
      onClick={onClick}
      disabled={state === "checking" || state === "downloading"}
      title="Check for PatchPilot updates"
    >
      {m.text}
    </button>
  );
}

function Count({ color, n, label }: { color: string; n: number; label: string }) {
  return (
    <span className="count">
      <span className="dot" style={{ background: color }} />
      {n} {label}
    </span>
  );
}

function tally(cards: ComponentStatus[]) {
  let ok = 0, warn = 0, fail = 0, skip = 0;
  for (const c of cards) {
    if (c.status === "Success") ok++;
    else if (c.status === "Warning") warn++;
    else if (c.status === "Failed") fail++;
    else if (c.status === "Skipped") skip++;
  }
  return { ok, warn, fail, skip };
}

function chipList(s: SystemInfo): string[] {
  const out: string[] = [];
  if (s.isDell) out.push("Dell");
  if (s.isSurface) out.push("Surface");
  if (s.hasNvidia) out.push("Nvidia");
  if (s.hasIntelGpu) out.push("Intel GPU");
  if (s.appRazer) out.push("Razer");
  if (s.appLogitech) out.push("Logitech");
  return out;
}

function tidy(mfr: string): string {
  return mfr.replace(/\s*(Inc\.?|Corporation|Ltd\.?|LLC)\s*$/i, "").trim();
}

function shortOs(os: string): string {
  if (/10\.0\.2[26]/.test(os)) return "Windows 11";
  if (/windows/i.test(os)) return "Windows";
  return os;
}

function modeLabel(m: RunMode): string {
  return m === "All" ? "Firmware + Software" : m === "Software" ? "Software only" : "Firmware only";
}

function fmtDur(secs: number): string {
  const m = Math.floor(secs / 60);
  const r = secs % 60;
  return `${String(m).padStart(2, "0")}:${String(r).padStart(2, "0")}`;
}

function fmtWhen(iso: string): string {
  try {
    return new Date(iso).toLocaleString(undefined, {
      hour: "2-digit",
      minute: "2-digit",
      weekday: "short",
      day: "2-digit",
      month: "short",
    });
  } catch {
    return iso;
  }
}
