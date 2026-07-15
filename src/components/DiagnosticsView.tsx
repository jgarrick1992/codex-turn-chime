import { CheckCircle2, RefreshCw, XCircle } from "lucide-react";
import { useI18n } from "../i18n";
import type { Diagnostics } from "../types";

export function DiagnosticsView({ diagnostics, onRefresh }: { diagnostics: Diagnostics | null; onRefresh: () => void }) {
  const { t } = useI18n();
  const checks = diagnostics ? [
    { label: t("hookConfig"), ok: diagnostics.hook_installed, detail: diagnostics.hook_config_path },
    { label: t("helper"), ok: diagnostics.helper_found, detail: diagnostics.helper_found ? t("healthy") : t("unavailable") },
    { label: t("eventQueue"), ok: diagnostics.queue_readable, detail: diagnostics.app_data_dir },
    { label: t("database"), ok: diagnostics.database_ready, detail: diagnostics.app_data_dir },
    { label: t("watcher"), ok: !diagnostics.watcher_enabled || diagnostics.watcher_compatible, detail: diagnostics.watcher_message || (diagnostics.watcher_enabled ? "codex-jsonl-v1" : "Disabled") },
    { label: t("dismissReminderShortcut"), ok: !diagnostics.shortcut_configured || diagnostics.shortcut_registered, detail: diagnostics.shortcut_configured ? diagnostics.shortcut_message || diagnostics.shortcut || t("shortcutInactive") : t("shortcutDisabled") },
  ] : [];
  return (
    <main className="diagnostics-page">
      <header className="page-title"><div><span className="eyebrow">Local health</span><h1>{t("diagnostics")}</h1></div><button className="secondary-button" type="button" onClick={onRefresh}><RefreshCw size={16} />{t("refresh")}</button></header>
      <section className="diagnostic-summary"><div className="summary-ring">{checks.filter((check) => check.ok).length}<small>/{checks.length}</small></div><div><h2>{t("health")}</h2><p>Checks are local and contain no conversation content.</p></div></section>
      <div className="diagnostic-list">{checks.map((check) => <div className="diagnostic-row" key={check.label}>{check.ok ? <CheckCircle2 className="check-ok" /> : <XCircle className="check-bad" />}<div><strong>{check.label}</strong><code>{check.detail}</code></div><span>{check.ok ? t("healthy") : t("unavailable")}</span></div>)}</div>
      {diagnostics && <section className="path-list"><h2>Paths</h2><dl><dt>{t("appData")}</dt><dd>{diagnostics.app_data_dir}</dd><dt>{t("codexHome")}</dt><dd>{diagnostics.codex_home}</dd><dt>{t("hookConfig")}</dt><dd>{diagnostics.hook_config_path}</dd></dl></section>}
    </main>
  );
}
