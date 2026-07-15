import { Check, Copy, FileText, Folder, Webhook } from "lucide-react";
import { useState } from "react";
import { useI18n } from "../i18n";
import type { MonitorEvent, TaskState } from "../types";
import { StatusDot } from "./StatusDot";

function MetaRow({ icon, label, value }: { icon: React.ReactNode; label: string; value: string }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    await navigator.clipboard.writeText(value);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1200);
  };
  return (
    <div className="meta-row">
      <span className="meta-label">{icon}{label}</span>
      <span className="meta-value" title={value}>{value}</span>
      <button type="button" className="icon-button" onClick={copy} aria-label={`Copy ${label}`}>{copied ? <Check size={14} /> : <Copy size={14} />}</button>
    </div>
  );
}

export function TaskDetail({ task, events, onMarkRead }: { task: TaskState | null; events: MonitorEvent[]; onMarkRead: () => void }) {
  const { language, t } = useI18n();
  if (!task) return <aside className="detail-panel detail-empty"><p>{t("noSelection")}</p></aside>;
  const sourceIcon = task.source === "codex_hook" ? <Webhook size={15} /> : <FileText size={15} />;
  return (
    <aside className="detail-panel">
      <header className="detail-header">
        <div><StatusDot kind={task.current_kind} /><h2>{t(task.current_kind === "needs_input" ? "needsInput" : task.current_kind)}</h2></div>
        {!task.is_read && <button className="secondary-button" type="button" onClick={onMarkRead}><Check size={16} />{t("markRead")}</button>}
      </header>
      <section>
        <h3>{t("identifiers")}</h3>
        <MetaRow icon={<span>#</span>} label={t("sessionId")} value={task.session_id} />
        <MetaRow icon={<span>#</span>} label={t("turnId")} value={task.turn_id} />
      </section>
      <section>
        <h3>{t("context")}</h3>
        <MetaRow icon={<Folder size={15} />} label={t("cwd")} value={task.cwd} />
        <MetaRow icon={sourceIcon} label={t("source")} value={task.source === "codex_hook" ? t("hook") : t("transcript")} />
        {task.reason && <div className="reason-box"><b>{t("reason")}</b><code>{task.reason}</code></div>}
        <p className="privacy-copy">{t("privacyDetail")}</p>
      </section>
      <section className="timeline-section">
        <h3>{t("eventTimeline")}</h3>
        <ol className="timeline">
          {events.map((event) => (
            <li key={event.event_id}>
              <StatusDot kind={event.kind} />
              <time>{new Intl.DateTimeFormat(language, { hour: "2-digit", minute: "2-digit", second: "2-digit" }).format(new Date(event.occurred_at))}</time>
              <div><strong>{t(event.kind === "needs_input" ? "needsInput" : event.kind)}</strong>{event.reason && <code>{event.reason}</code>}</div>
            </li>
          ))}
        </ol>
      </section>
    </aside>
  );
}
