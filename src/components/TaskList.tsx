import { ChevronRight, FileText, Webhook } from "lucide-react";
import { useI18n } from "../i18n";
import type { TaskState } from "../types";
import { StatusDot } from "./StatusDot";

function taskName(cwd: string): string {
  const cleaned = cwd.replace(/[\\/]+$/, "");
  return cleaned.split(/[\\/]/).pop() || cwd;
}

function formatTime(value: string, language: string): string {
  return new Intl.DateTimeFormat(language, { hour: "2-digit", minute: "2-digit", second: "2-digit" }).format(new Date(value));
}

export function TaskList({ tasks, selected, onSelect }: { tasks: TaskState[]; selected: TaskState | null; onSelect: (task: TaskState) => void }) {
  const { language, t } = useI18n();
  if (tasks.length === 0) {
    return (
      <div className="empty-state">
        <div className="empty-wave">⌁</div>
        <h2>{t("noTasks")}</h2>
        <p>{t("noTasksHint")}</p>
      </div>
    );
  }
  return (
    <div className="task-table">
      <div className="task-table-head">
        <span>{t("status")}</span><span>{t("task")}</span><span>{t("source")}</span><span>{t("lastEvent")}</span><span />
      </div>
      {tasks.map((task) => {
        const isSelected = selected?.session_id === task.session_id && selected.turn_id === task.turn_id;
        const SourceIcon = task.source === "codex_hook" ? Webhook : FileText;
        return (
          <button className={isSelected ? "task-row selected" : "task-row"} key={`${task.session_id}:${task.turn_id}`} onClick={() => onSelect(task)} type="button">
            <span><StatusDot kind={task.current_kind} pulse={task.current_kind === "running"} /></span>
            <span className="task-primary"><strong>{taskName(task.cwd)}</strong><small>{task.cwd}</small></span>
            <span className="task-source"><SourceIcon size={16} />{task.source === "codex_hook" ? t("hook") : t("transcript")}</span>
            <time>{formatTime(task.last_event_at, language)}</time>
            <span className="row-tail">{!task.is_read && <i aria-label="Unread" />}<ChevronRight size={17} /></span>
          </button>
        );
      })}
    </div>
  );
}
