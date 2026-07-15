import { open } from "@tauri-apps/plugin-dialog";
import { BellRing, CheckCircle2, FileAudio, FlaskConical, Laptop, Link2, Play, ShieldCheck, Volume2 } from "lucide-react";
import { useState } from "react";
import { useI18n } from "../i18n";
import type { AppSettings, HookPreview, MonitorKind, SoundSetting } from "../types";

type Tab = "general" | "sounds" | "integration" | "privacy";

function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (checked: boolean) => void; label: string }) {
  return (
    <button className={checked ? "toggle on" : "toggle"} role="switch" aria-checked={checked} aria-label={label} onClick={() => onChange(!checked)} type="button">
      <span />
    </button>
  );
}

function SoundEditor({
  kind,
  value,
  onChange,
  onTest,
}: {
  kind: "needs_input" | "ready";
  value: SoundSetting;
  onChange: (value: SoundSetting) => void;
  onTest: (kind: MonitorKind) => void;
}) {
  const { t } = useI18n();
  const choose = async () => {
    const selected = await open({ multiple: false, directory: false, filters: [{ name: "Audio", extensions: ["wav", "mp3"] }] });
    if (selected) onChange({ ...value, path: selected });
  };
  return (
    <div className="sound-editor">
      <div className="setting-icon">{kind === "needs_input" ? <BellRing /> : <CheckCircle2 />}</div>
      <div className="sound-main">
        <div className="setting-heading"><strong>{kind === "needs_input" ? t("needsInput") : t("ready")}</strong><Toggle checked={value.enabled} onChange={(enabled) => onChange({ ...value, enabled })} label={t("enabled")} /></div>
        <button className="file-picker" type="button" onClick={choose}><FileAudio size={16} /><span>{value.path || t("defaultSound")}</span></button>
        <div className="volume-row"><Volume2 size={16} /><input aria-label={t("volume")} type="range" min="0" max="1" step="0.05" value={value.volume} onChange={(event) => onChange({ ...value, volume: Number(event.target.value) })} /><span>{Math.round(value.volume * 100)}%</span><button className="secondary-button compact" type="button" onClick={() => onTest(kind)}><Play size={15} />{t("test")}</button></div>
      </div>
    </div>
  );
}

export function HookDiffDialog({ preview, installing, onClose, onConfirm }: { preview: HookPreview; installing: boolean; onClose: () => void; onConfirm: () => void }) {
  const { t } = useI18n();
  return (
    <div className="modal-backdrop" role="presentation">
      <section className="modal" role="dialog" aria-modal="true" aria-labelledby="hook-diff-title">
        <header><div><small>{preview.config_path}</small><h2 id="hook-diff-title">{t("confirmInstall")}</h2><p>{t("confirmInstallHelp")}</p></div><button className="icon-button" onClick={onClose} type="button" aria-label={t("close")}>×</button></header>
        <pre className="diff-view">{preview.diff}</pre>
        <footer><button className="secondary-button" type="button" onClick={onClose}>{t("cancel")}</button><button className="primary-button" disabled={installing || preview.already_installed} type="button" onClick={onConfirm}>{preview.already_installed ? t("alreadyInstalled") : t("confirm")}</button></footer>
      </section>
    </div>
  );
}

export function SettingsView({
  settings,
  hookInstalled,
  onSave,
  onTestSound,
  onPreviewHook,
  onUninstallHook,
}: {
  settings: AppSettings;
  hookInstalled: boolean;
  onSave: (settings: AppSettings) => void;
  onTestSound: (kind: MonitorKind) => void;
  onPreviewHook: () => void;
  onUninstallHook: () => void;
}) {
  const { language, setLanguage, t } = useI18n();
  const [tab, setTab] = useState<Tab>("general");
  const update = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => onSave({ ...settings, [key]: value });
  const tabs: Array<{ key: Tab; icon: typeof Laptop }> = [
    { key: "general", icon: Laptop },
    { key: "sounds", icon: Volume2 },
    { key: "integration", icon: Link2 },
    { key: "privacy", icon: ShieldCheck },
  ];
  return (
    <main className="settings-page">
      <header className="page-title"><div><span className="eyebrow">CodexTurnChime</span><h1>{t("settings")}</h1></div></header>
      <div className="settings-layout">
        <nav className="settings-tabs">{tabs.map(({ key, icon: Icon }) => <button key={key} type="button" className={tab === key ? "active" : ""} onClick={() => setTab(key)}><Icon size={17} />{t(key)}</button>)}</nav>
        <section className="settings-content">
          {tab === "general" && (
            <>
              <div className="settings-section"><h2>{t("general")}</h2><div className="setting-row"><div><strong>{t("language")}</strong><p>English / 简体中文</p></div><select value={language} onChange={(event) => { const next = event.target.value as "en" | "zh-CN"; setLanguage(next); update("language", next); }}><option value="en">English</option><option value="zh-CN">简体中文</option></select></div></div>
              <div className="settings-section"><div className="setting-row"><div><strong>{t("launchAtLogin")}</strong><p>{t("launchAtLoginHelp")}</p></div><Toggle checked={settings.launch_at_login} onChange={(checked) => update("launch_at_login", checked)} label={t("launchAtLogin")} /></div></div>
            </>
          )}
          {tab === "sounds" && (
            <div className="settings-section"><h2>{t("sounds")}</h2><SoundEditor kind="needs_input" value={settings.needs_input_sound} onChange={(value) => update("needs_input_sound", value)} onTest={onTestSound} /><SoundEditor kind="ready" value={settings.ready_sound} onChange={(value) => update("ready_sound", value)} onTest={onTestSound} /></div>
          )}
          {tab === "integration" && (
            <>
              <div className="settings-section integration-card"><div className="setting-icon"><Link2 /></div><div><h2>{t("hookIntegration")}</h2><p>{t("hookHelp")}</p><span className={hookInstalled ? "health-badge ok" : "health-badge"}>{hookInstalled ? t("alreadyInstalled") : t("notInstalled")}</span><div className="button-row"><button className="primary-button" type="button" onClick={onPreviewHook}>{t("previewChanges")}</button>{hookInstalled && <button className="secondary-button danger" type="button" onClick={onUninstallHook}>{t("uninstallHook")}</button>}</div></div></div>
              <div className="settings-section integration-card experimental"><div className="setting-icon"><FlaskConical /></div><div><div className="setting-heading"><h2>{t("transcriptWatcher")}</h2><Toggle checked={settings.transcript_watcher_enabled} onChange={(checked) => update("transcript_watcher_enabled", checked)} label={t("transcriptWatcher")} /></div><p>{t("transcriptWatcherHelp")}</p><code>codex-jsonl-v1 · fail-closed</code></div></div>
            </>
          )}
          {tab === "privacy" && (
            <div className="settings-section privacy-panel"><ShieldCheck size={32} /><h2>{t("localOnly")}</h2><p>{t("privacyPromise")}</p><ul><li>SQLite stores only MonitorEvent v1 metadata.</li><li>No prompt, answer, command, input, or output fields exist in the schema.</li><li>History is deleted after 30 days and can be cleared immediately.</li></ul></div>
          )}
        </section>
      </div>
    </main>
  );
}
