// Small shared pieces for the Tasks views: the board columns, and the chips
// (priority, due date, assignee avatar, source link) that read the same task
// fields whether they render in a card or a list row.
import { Calendar, Link2 } from "lucide-react";

import { Avatar as DsAvatar, Badge } from "../ds";
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

/** A priority as a `ds/Badge`. The three steps map onto the design system's
 *  tones — high is danger, medium is warning (the tone D2.11 added for it),
 *  low is neutral. The two hand-drawn versions of this (here and ListView's)
 *  disagreed on low — one greenish, one grey — which was an accident, not a
 *  decision; neutral is the reconciliation. */
export function PriorityChip({ priority }: { priority: TaskPriority }) {
  if (priority === "none") return null;
  const tone = priority === "high" ? "danger" : priority === "medium" ? "warning" : "neutral";
  const label =
    priority === "high"
      ? strings.taskPrioHigh
      : priority === "medium"
        ? strings.taskPrioMedium
        : strings.taskPrioLow;
  return <Badge tone={tone}>{label}</Badge>;
}

export function DueChip({ iso, done }: { iso: string; done: boolean }) {
  return (
    <span className={`inline-flex items-center gap-1 text-xs ${!done && isOverdue(iso) ? "text-danger" : "text-secondary"}`}>
      <Calendar size={12} /> {dueLabel(iso)}
    </span>
  );
}

/** The assignee's mark, drawn by `ds/Avatar` (D2.11 — the local seven-colour
 *  hash it replaces was this module's own palette in hard-coded hex). The name
 *  it takes initials from is derived from the email's local part
 *  ("jane.doe@…" → "jane doe"), which keeps the two-letter initials the local
 *  version drew; the email keys the tint, so a person keeps one colour. */
export function Avatar({ email }: { email: string }) {
  const local = email.split("@")[0] ?? email;
  const name = local.split(/[.\-_]+/).filter(Boolean).join(" ") || email;
  return <DsAvatar name={name} email={email} size="sm" title={email} />;
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
