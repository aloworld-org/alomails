// Small shared pieces for the Tasks views: the board columns, and the chips
// (priority, due date, assignee avatar, source link) that read the same task
// fields whether they render in a card or a list row.
import { Calendar, Link2 } from "lucide-react";

import { strings } from "../i18n";
import type { Task, TaskLabelDto, TaskPriority } from "../jmap";

/** A default palette offered when creating a label without a chosen colour. */
export const LABEL_PALETTE = ["#e76f51", "#4b83c4", "#2e8b57", "#9b6dd6", "#e0a63b", "#d1568f", "#3aa8a0"];

/** A task's labels as tinted pills. */
export function LabelChips({ labels }: { labels?: TaskLabelDto[] | undefined }) {
  if (labels === undefined || labels.length === 0) return null;
  return (
    <span className="inline-flex flex-wrap items-center gap-1">
      {labels.map((l) => (
        <span
          key={l.id}
          className="whitespace-nowrap rounded-full px-2 py-0.5 text-[0.68rem] font-medium"
          style={{
            background: `color-mix(in srgb, ${l.color ?? "var(--accent)"} 15%, transparent)`,
            color: `color-mix(in srgb, ${l.color ?? "var(--accent)"} 72%, var(--text-primary))`,
          }}
        >
          {l.name}
        </span>
      ))}
    </span>
  );
}

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
  const cls = priority === "high"
    ? "bg-[#fbe0d8] text-[#b23c22]"
    : priority === "medium"
      ? "bg-[#fdf0d8] text-[#9a6a12]"
      : "bg-[#eef6f2] text-secondary";
  const label =
    priority === "high"
      ? strings.taskPrioHigh
      : priority === "medium"
        ? strings.taskPrioMedium
        : strings.taskPrioLow;
  return <span className={`rounded-full px-2 py-0.5 text-[11px] font-semibold capitalize ${cls}`}>{label}</span>;
}

export function DueChip({ iso, done }: { iso: string; done: boolean }) {
  return (
    <span className={`inline-flex items-center gap-1 text-xs ${!done && isOverdue(iso) ? "text-danger" : "text-secondary"}`}>
      <Calendar size={12} /> {dueLabel(iso)}
    </span>
  );
}

const AVATAR_COLORS = ["#e76f51", "#4b83c4", "#2e8b57", "#9b6dd6", "#e0a63b", "#d1568f", "#3aa8a0"];

/** A stable colour per person, so assignees read as distinct at a glance. */
function avatarColor(email: string): string {
  let h = 0;
  for (const ch of email) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
  return AVATAR_COLORS[h % AVATAR_COLORS.length] ?? "#e76f51";
}

export function Avatar({ email }: { email: string }) {
  return (
    <span className="inline-flex size-[22px] shrink-0 items-center justify-center rounded-full text-[10px] font-semibold text-white" style={{ background: avatarColor(email) }} title={email}>
      {initials(email)}
    </span>
  );
}

/** The link-back marker shown when a task came from an email or event. */
export function SourceMark({ task }: { task: Task }) {
  if (!task.sourceKind) return null;
  const label = task.sourceKind === "email" ? strings.taskFromEmail : strings.taskFromEvent;
  return (
    <span className="inline-flex items-center gap-1 text-xs text-accent" title={label}>
      <Link2 size={13} />
    </span>
  );
}
