// Small shared pieces for the Tasks views: the board columns, and the chips
// (priority, due date, assignee avatar, source link) that read the same task
// fields whether they render in a card or a list row.
import { Calendar, Link2 } from "lucide-react";

import { strings } from "../i18n";
import type { Task, TaskPriority } from "../jmap";
import styles from "./TasksModule.module.css";

/** The board's default columns (the task `status` values). Status is free text
 *  in the store, so this list defines the ordered, named workflow the UI shows. */
export const COLUMNS: { key: string; label: () => string }[] = [
  { key: "todo", label: () => strings.taskColTodo },
  { key: "in_progress", label: () => strings.taskColInProgress },
  { key: "review", label: () => strings.taskColReview },
  { key: "done", label: () => strings.taskColDone },
];

export function columnLabel(status: string): string {
  return COLUMNS.find((c) => c.key === status)?.label() ?? status;
}

/** A colour per workflow status — shared by the board, list, and timeline so a
 *  task reads the same everywhere (coral → blue → violet → green). */
export const STATUS_COLORS: Record<string, string> = {
  todo: "#e76f51",
  in_progress: "#4b83c4",
  review: "#9b6dd6",
  done: "#2e8b57",
};

export function statusColor(status: string): string {
  return STATUS_COLORS[status] ?? "#e76f51";
}

/** Two-letter initials from an email/name for the assignee avatar. */
export function initials(email: string): string {
  const name = email.split("@")[0] ?? email;
  const parts = name.split(/[.\-_]+/).filter(Boolean);
  const first = parts[0]?.[0] ?? name[0] ?? "?";
  const second = parts[1]?.[0] ?? "";
  return (first + second).toUpperCase();
}

/** A friendly due-date label; today/tomorrow spelled out. */
export function dueLabel(iso: string): string {
  const d = new Date(iso);
  const today = new Date();
  const startOfDay = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate());
  const days = Math.round((startOfDay(d).getTime() - startOfDay(today).getTime()) / 86400000);
  if (days === 0) return strings.taskDueToday;
  if (days === 1) return strings.taskDueTomorrow;
  if (days === -1) return strings.taskDueYesterday;
  return d.toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

export function isOverdue(iso: string): boolean {
  return new Date(iso).getTime() < Date.now();
}

export function PriorityChip({ priority }: { priority: TaskPriority }) {
  if (priority === "none") return null;
  const cls =
    priority === "high" ? styles.prioHigh : priority === "medium" ? styles.prioMedium : styles.prioLow;
  const label =
    priority === "high"
      ? strings.taskPrioHigh
      : priority === "medium"
        ? strings.taskPrioMedium
        : strings.taskPrioLow;
  return <span className={`${styles.priority} ${cls}`}>{label}</span>;
}

export function DueChip({ iso, done }: { iso: string; done: boolean }) {
  return (
    <span className={`${styles.due} ${!done && isOverdue(iso) ? styles.dueOverdue : ""}`}>
      <Calendar size={12} /> {dueLabel(iso)}
    </span>
  );
}

export function Avatar({ email }: { email: string }) {
  return (
    <span className={styles.avatar} title={email}>
      {initials(email)}
    </span>
  );
}

/** The link-back marker shown when a task came from an email or event. */
export function SourceMark({ task }: { task: Task }) {
  if (!task.sourceKind) return null;
  const label = task.sourceKind === "email" ? strings.taskFromEmail : strings.taskFromEvent;
  return (
    <span className={`${styles.metaIcon} ${styles.sourceIcon}`} title={label}>
      <Link2 size={13} />
    </span>
  );
}
