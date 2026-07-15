import type { MonitorKind } from "../types";

export function StatusDot({ kind, pulse = false }: { kind: MonitorKind; pulse?: boolean }) {
  return <span className={`status-dot status-${kind}${pulse ? " status-pulse" : ""}`} aria-hidden="true" />;
}
