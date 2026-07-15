import { BellOff, BellRing, Languages, Search, ShieldCheck, Trash2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { bridge, isTauriRuntime } from "./bridge";
import { DiagnosticsView } from "./components/DiagnosticsView";
import { HookDiffDialog, SettingsView } from "./components/SettingsView";
import { Onboarding } from "./components/Onboarding";
import { Sidebar } from "./components/Sidebar";
import { TaskDetail } from "./components/TaskDetail";
import { TaskList } from "./components/TaskList";
import { useI18n } from "./i18n";
import type { AppSettings, Diagnostics, HookPreview, MonitorEvent, MonitorKind, Section, TaskState } from "./types";

const DEFAULT_SETTINGS: AppSettings = {
  language: "en",
  muted: false,
  launch_at_login: false,
  transcript_watcher_enabled: false,
  history_retention_days: 30,
  onboarding_complete: false,
  needs_input_sound: { enabled: true, path: null, volume: 0.7 },
  ready_sound: { enabled: true, path: null, volume: 0.58 },
};

function playGeneratedChime(kind: MonitorKind, volume: number): void {
  const context = new AudioContext();
  const gain = context.createGain();
  gain.gain.setValueAtTime(0, context.currentTime);
  gain.gain.linearRampToValueAtTime(volume * 0.22, context.currentTime + 0.015);
  gain.gain.exponentialRampToValueAtTime(0.001, context.currentTime + 0.55);
  gain.connect(context.destination);
  const frequencies = kind === "needs_input" ? [660, 520] : [520, 780];
  frequencies.forEach((frequency, index) => {
    const oscillator = context.createOscillator();
    oscillator.type = "sine";
    oscillator.frequency.value = frequency;
    oscillator.connect(gain);
    oscillator.start(context.currentTime + index * 0.16);
    oscillator.stop(context.currentTime + 0.34 + index * 0.16);
  });
  window.setTimeout(() => void context.close(), 800);
}

export default function App() {
  const { language, setLanguage, t } = useI18n();
  const [section, setSection] = useState<Section>("all");
  const [tasks, setTasks] = useState<TaskState[]>([]);
  const [selected, setSelected] = useState<TaskState | null>(null);
  const [events, setEvents] = useState<MonitorEvent[]>([]);
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [diagnostics, setDiagnostics] = useState<Diagnostics | null>(null);
  const [hookPreview, setHookPreview] = useState<HookPreview | null>(null);
  const [installing, setInstalling] = useState(false);
  const [search, setSearch] = useState("");
  const [error, setError] = useState<string | null>(null);
  const settingsRef = useRef(settings);
  settingsRef.current = settings;

  const refreshTasks = useCallback(async () => {
    if (!isTauriRuntime()) return;
    const next = await bridge.listTasks();
    setTasks(next);
    setSelected((current) => current ? next.find((item) => item.session_id === current.session_id && item.turn_id === current.turn_id) || next[0] || null : next[0] || null);
  }, []);

  const refreshDiagnostics = useCallback(async () => {
    if (!isTauriRuntime()) return;
    setDiagnostics(await bridge.diagnostics());
  }, []);

  useEffect(() => {
    if (!isTauriRuntime()) {
      setError(t("bridgeUnavailable"));
      setSettings({ ...DEFAULT_SETTINGS, onboarding_complete: true });
      return;
    }
    void Promise.all([bridge.getSettings(), bridge.listTasks(), bridge.diagnostics()]).then(([nextSettings, nextTasks, nextDiagnostics]) => {
      setSettings(nextSettings);
      setLanguage(nextSettings.language);
      setTasks(nextTasks);
      setSelected(nextTasks[0] || null);
      setDiagnostics(nextDiagnostics);
    }).catch((cause: unknown) => setError(String(cause)));
  }, [setLanguage, t]);

  const playSound = useCallback(async (kind: MonitorKind) => {
    if (kind !== "needs_input" && kind !== "ready") return;
    const active = kind === "needs_input" ? settingsRef.current.needs_input_sound : settingsRef.current.ready_sound;
    if (settingsRef.current.muted || !active.enabled) return;
    try {
      if (!active.path) {
        playGeneratedChime(kind, active.volume);
        return;
      }
      const payload = await bridge.readSound(active.path);
      const audio = new Audio(`data:${payload.mime};base64,${payload.base64}`);
      audio.volume = active.volume;
      await audio.play();
    } catch {
      setError(t("soundError"));
    }
  }, [t]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let unlisten: (() => void) | undefined;
    void bridge.onMonitorEvent(async (event) => {
      await refreshTasks();
      await playSound(event.kind);
      if (event.kind === "needs_input" || event.kind === "ready") {
        let granted = await isPermissionGranted();
        if (!granted) granted = (await requestPermission()) === "granted";
        if (granted) sendNotification({ title: event.kind === "needs_input" ? t("needsInput") : t("ready"), body: event.cwd });
      }
    }).then((cleanup) => { unlisten = cleanup; });
    return () => unlisten?.();
  }, [playSound, refreshTasks, t]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    const cleanups: Array<() => void> = [];
    void bridge.onSettingsChanged((next) => {
      setSettings(next);
      setLanguage(next.language);
    }).then((cleanup) => cleanups.push(cleanup));
    void bridge.onWatcherDisabled((message) => setError(message)).then((cleanup) => cleanups.push(cleanup));
    return () => cleanups.forEach((cleanup) => cleanup());
  }, [setLanguage]);

  useEffect(() => {
    if (!selected || !isTauriRuntime()) { setEvents([]); return; }
    void bridge.listEvents(selected.session_id, selected.turn_id).then(setEvents).catch((cause: unknown) => setError(String(cause)));
  }, [selected]);

  const saveSettings = async (next: AppSettings) => {
    setSettings(next);
    setLanguage(next.language);
    if (!isTauriRuntime()) return;
    try { setSettings(await bridge.saveSettings(next)); } catch (cause) { setError(String(cause)); }
  };

  const previewHook = async () => {
    try { setHookPreview(await bridge.previewHook()); } catch (cause) { setError(String(cause)); }
  };

  const installHook = async () => {
    setInstalling(true);
    try { setHookPreview(await bridge.installHook()); await refreshDiagnostics(); } catch (cause) { setError(String(cause)); } finally { setInstalling(false); }
  };

  const uninstallHook = async () => {
    try { await bridge.uninstallHook(); await refreshDiagnostics(); } catch (cause) { setError(String(cause)); }
  };

  const filteredTasks = useMemo(() => tasks.filter((task) => {
    const matchesSection = section === "all" || section === "settings" || section === "diagnostics" || task.current_kind === section;
    const needle = search.trim().toLowerCase();
    return matchesSection && (!needle || `${task.cwd} ${task.session_id} ${task.turn_id}`.toLowerCase().includes(needle));
  }), [tasks, section, search]);

  if (!settings.onboarding_complete) return <><Onboarding settings={settings} hookInstalled={Boolean(diagnostics?.hook_installed)} onPreviewHook={previewHook} onTestSound={playSound} onFinish={saveSettings} />{hookPreview && <HookDiffDialog preview={hookPreview} installing={installing} onClose={() => setHookPreview(null)} onConfirm={installHook} />}</>;

  return (
    <div className="app-shell">
      <Sidebar section={section} tasks={tasks} onSelect={setSection} />
      <div className="workspace">
        {section === "settings" ? <SettingsView settings={settings} hookInstalled={Boolean(diagnostics?.hook_installed)} onSave={saveSettings} onTestSound={playSound} onPreviewHook={previewHook} onUninstallHook={uninstallHook} /> : section === "diagnostics" ? <DiagnosticsView diagnostics={diagnostics} onRefresh={refreshDiagnostics} /> : <>
          <header className="topbar"><div><span className="eyebrow">CodexTurnChime</span><h1>{t("taskMonitor")}</h1></div><div className="topbar-actions"><span className="local-chip"><ShieldCheck size={15} />{t("localOnly")}</span><label className="search-box"><Search size={16} /><input value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t("search")} /></label><button className="icon-button language-button" type="button" onClick={() => void saveSettings({ ...settings, language: language === "en" ? "zh-CN" : "en" })}><Languages size={18} /><span>{language === "en" ? "EN" : "中"}</span></button><button className="icon-button" type="button" onClick={() => void saveSettings({ ...settings, muted: !settings.muted })} aria-label={settings.muted ? t("unmute") : t("mute")}>{settings.muted ? <BellOff size={19} /> : <BellRing size={19} />}</button></div></header>
          <div className="monitor-grid"><section className="task-pane"><div className="task-actions"><span>{filteredTasks.length} {t("task").toLowerCase()}</span><button className="text-button danger" type="button" onClick={async () => { if (window.confirm(t("deleteConfirm"))) { await bridge.clearHistory(); await refreshTasks(); } }}><Trash2 size={15} />{t("clearHistory")}</button></div><TaskList tasks={filteredTasks} selected={selected} onSelect={setSelected} /></section><TaskDetail task={selected} events={events} onMarkRead={async () => { if (!selected) return; await bridge.markRead(selected.session_id, selected.turn_id); await refreshTasks(); }} /></div>
          <footer className="retention-footer">{t("historyRetention")}</footer>
        </>}
      </div>
      {error && <div className="toast error-toast" role="alert"><span>{error}</span><button type="button" onClick={() => setError(null)}>×</button></div>}
      {hookPreview && <HookDiffDialog preview={hookPreview} installing={installing} onClose={() => setHookPreview(null)} onConfirm={installHook} />}
    </div>
  );
}
