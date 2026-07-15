import {
  Activity,
  BellRing,
  CheckCircle2,
  CircleHelp,
  CircleStop,
  ListTodo,
  Settings,
  ShieldAlert,
  TriangleAlert,
} from "lucide-react";
import { useI18n, type TranslationKey } from "../i18n";
import type { MonitorKind, Section, TaskState } from "../types";
import { BrandMark } from "./BrandMark";

const statusItems: Array<{ section: Section; label: TranslationKey; icon: typeof Activity }> = [
  { section: "all", label: "allTasks", icon: ListTodo },
  { section: "needs_input", label: "needsInput", icon: BellRing },
  { section: "blocked", label: "blocked", icon: ShieldAlert },
  { section: "running", label: "running", icon: Activity },
  { section: "ready", label: "ready", icon: CheckCircle2 },
  { section: "stopped", label: "stopped", icon: CircleStop },
  { section: "unknown", label: "unknown", icon: CircleHelp },
];

export function Sidebar({
  section,
  tasks,
  onSelect,
}: {
  section: Section;
  tasks: TaskState[];
  onSelect: (section: Section) => void;
}) {
  const { t } = useI18n();
  const count = (kind: MonitorKind) => tasks.filter((task) => task.current_kind === kind).length;
  return (
    <aside className="sidebar">
      <BrandMark />
      <nav aria-label="Task filters">
        {statusItems.map(({ section: itemSection, label, icon: Icon }) => {
          const itemCount = itemSection === "all" ? tasks.length : count(itemSection as MonitorKind);
          return (
            <button
              className={section === itemSection ? "nav-item active" : "nav-item"}
              key={itemSection}
              onClick={() => onSelect(itemSection)}
              type="button"
            >
              <Icon size={18} strokeWidth={1.8} />
              <span>{t(label)}</span>
              {itemCount > 0 && <b>{itemCount}</b>}
            </button>
          );
        })}
      </nav>
      <div className="nav-secondary">
        <button className={section === "settings" ? "nav-item active" : "nav-item"} onClick={() => onSelect("settings")} type="button">
          <Settings size={18} /> <span>{t("settings")}</span>
        </button>
        <button className={section === "diagnostics" ? "nav-item active" : "nav-item"} onClick={() => onSelect("diagnostics")} type="button">
          <TriangleAlert size={18} /> <span>{t("diagnostics")}</span>
        </button>
      </div>
      <div className="local-footnote">
        <span className="privacy-led" />
        <span>{t("localOnly")}</span>
      </div>
    </aside>
  );
}
