// The List view's real filter / sort / group configuration. All of it operates
// on the tasks already loaded from the API — no fake data — so the toolbar just
// reshapes what's there. Timeline and Calendar reuse the filter half.
import { strings } from "../i18n";
import type { Task, TaskPriority } from "../jmap";
import { COLUMNS, columnLabel } from "./parts";

export type SortKey = "manual" | "due" | "priority" | "name" | "created";
export type GroupKey = "status" | "project" | "assignee" | "priority" | "none";

export interface ViewConfig {
  sort: SortKey;
  group: GroupKey;
  /** Selected priorities to show; empty = all. */
  priorities: ReadonlySet<TaskPriority>;
  /** Show only tasks assigned to the current user. */
  onlyMine: boolean;
  /** Include completed tasks. */
  showCompleted: boolean;
  /** Denser rows. */
  compact: boolean;
}

export const DEFAULT_CONFIG: ViewConfig = {
  sort: "manual",
  group: "status",
  priorities: new Set(),
  onlyMine: false,
  showCompleted: true,
  compact: false,
};

/** True when any control is set away from its default (drives the "active" dot). */
export function isFiltering(c: ViewConfig): boolean {
  return c.priorities.size > 0 || c.onlyMine || !c.showCompleted;
}

const PRIO_RANK: Record<string, number> = { high: 0, medium: 1, low: 2, none: 3 };

/** Filter to the tasks the config admits (priority, assignee, completed). */
export function filterTasks(tasks: Task[], c: ViewConfig, me?: string): Task[] {
  return tasks.filter((t) => {
    if (c.priorities.size > 0 && !c.priorities.has(t.priority)) return false;
    if (c.onlyMine && (me === undefined || (t.assignee ?? "").toLowerCase() !== me.toLowerCase())) {
      return false;
    }
    if (!c.showCompleted && t.status === "done") return false;
    return true;
  });
}

/** Order tasks by the chosen sort (a copy; input untouched). */
export function sortTasks(tasks: Task[], c: ViewConfig): Task[] {
  const list = [...tasks];
  switch (c.sort) {
    case "due":
      return list.sort(
        (a, b) => (a.dueAt ?? "9999").localeCompare(b.dueAt ?? "9999") || a.position - b.position,
      );
    case "priority":
      return list.sort(
        (a, b) => (PRIO_RANK[a.priority] ?? 3) - (PRIO_RANK[b.priority] ?? 3) || a.position - b.position,
      );
    case "name":
      return list.sort((a, b) => a.title.localeCompare(b.title));
    case "created":
      return list.sort((a, b) => b.createdAt.localeCompare(a.createdAt));
    default:
      return list.sort((a, b) => a.position - b.position);
  }
}

export interface Group {
  key: string;
  label: string;
  /** The status column key when grouping by status (enables the inline add row). */
  status?: string;
  items: Task[];
}

interface GroupCtx {
  projectName: (id: string) => string;
  me?: string | undefined;
}

function assigneeShort(email: string, me?: string): string {
  if (me !== undefined && email.toLowerCase() === me.toLowerCase()) return strings.taskAssigneeYou;
  const local = email.split("@")[0] ?? email;
  const first = local.split(/[.\-_]+/)[0] ?? local;
  return first.charAt(0).toUpperCase() + first.slice(1);
}

/** Split ordered tasks into labelled groups per the config's grouping. */
export function groupTasks(tasks: Task[], c: ViewConfig, ctx: GroupCtx): Group[] {
  if (c.group === "none") {
    return [{ key: "all", label: strings.taskAllTasks, items: tasks }];
  }
  if (c.group === "status") {
    return COLUMNS.map((col) => ({
      key: col.key,
      label: col.label(),
      status: col.key,
      items: tasks.filter((t) => t.status === col.key),
    }));
  }
  if (c.group === "priority") {
    const order: TaskPriority[] = ["high", "medium", "low", "none"];
    return order
      .map((p) => ({
        key: p,
        label:
          p === "high"
            ? strings.taskPrioHigh
            : p === "medium"
              ? strings.taskPrioMedium
              : p === "low"
                ? strings.taskPrioLow
                : strings.taskPrioNone,
        items: tasks.filter((t) => t.priority === p),
      }))
      .filter((g) => g.items.length > 0);
  }
  // project / assignee: derive present buckets, ordered by label.
  const buckets = new Map<string, Task[]>();
  for (const t of tasks) {
    const key =
      c.group === "project"
        ? t.projectId
        : (t.assignee ?? "").toLowerCase() || "__unassigned__";
    const arr = buckets.get(key);
    if (arr === undefined) buckets.set(key, [t]);
    else arr.push(t);
  }
  const labelFor = (key: string): string => {
    if (c.group === "project") return ctx.projectName(key) || strings.moduleTasks;
    if (key === "__unassigned__") return strings.taskUnassigned;
    return assigneeShort(key, ctx.me);
  };
  return [...buckets.entries()]
    .map(([key, items]) => ({ key, label: labelFor(key), items }))
    .sort((a, b) => a.label.localeCompare(b.label));
}

export { columnLabel };
