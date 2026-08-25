// The list view: a grouped table — the same task rows as the board (ADR 0022),
// here as columns (name · project · assignee · due · priority). The toolbar's
// filter / sort / group config reshapes the loaded tasks (viewConfig); groups
// collapse; status groups get an inline "add task" row. Checking the circle
// moves a task to/from Done via the one-field move the board uses.
import { useState } from "react";
import {
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Circle,
  Clock3,
  ListChecks,
  Plus,
} from "lucide-react";

import { strings } from "../i18n";
import type { Task } from "../jmap";
import { Avatar, LabelChips, dueLabel, isOverdue, statusColor } from "./parts";
import {
  filterTasks,
  groupTasks,
  sortTasks,
  type ViewConfig,
} from "./viewConfig";

interface Props {
  tasks: Task[];
  config: ViewConfig;
  projectName: (projectId: string) => string;
  me?: string | undefined;
  search?: string | undefined;
  onOpen: (id: string) => void;
  onMove: (id: string, status: string, position: number) => void;
  onAdd?: (status: string) => void;
}

function assigneeName(email: string, me?: string): string {
  if (me !== undefined && email.toLowerCase() === me.toLowerCase())
    return strings.taskAssigneeYou;
  const local = email.split("@")[0] ?? email;
  const first = local.split(/[.\-_]+/)[0] ?? local;
  return first.charAt(0).toUpperCase() + first.slice(1);
}

function PriorityCell({ task }: { task: Task }) {
  if (task.status === "done") {
    return (
      <span className="inline-flex items-center gap-1.5 rounded-full bg-[var(--success-tint)] px-2.5 py-1 text-xs font-semibold text-success">
        <CheckCircle2 size={13} aria-hidden="true" />
        {strings.taskColDone}
      </span>
    );
  }
  if (task.priority === "none") return <span className="text-tertiary">—</span>;
  const tone =
    task.priority === "high"
      ? "bg-[var(--danger-tint)] text-danger"
      : task.priority === "medium"
        ? "bg-[#fdf0d8] text-[#8a5a08]"
        : "bg-raised text-secondary";
  const label =
    task.priority === "high"
      ? strings.taskPrioHigh
      : task.priority === "medium"
        ? strings.taskPrioMedium
        : strings.taskPrioLow;
  return (
    <span
      className={`inline-flex items-center rounded-full px-2.5 py-1 text-xs font-semibold ${tone}`}
    >
      {label}
    </span>
  );
}

export function ListView({
  tasks,
  config,
  projectName,
  me,
  search,
  onOpen,
  onMove,
  onAdd,
}: Props) {
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());
  const q = (search ?? "").trim().toLowerCase();
  const searched =
    q === ""
      ? tasks
      : tasks.filter(
          (t) =>
            t.title.toLowerCase().includes(q) ||
            projectName(t.projectId).toLowerCase().includes(q),
        );
  const ordered = sortTasks(filterTasks(searched, config, me), config);
  const groups = groupTasks(ordered, config, { projectName, me });
  const completed = ordered.filter((task) => task.status === "done").length;
  const overdue = ordered.filter(
    (task) =>
      task.status !== "done" && task.dueAt !== null && isOverdue(task.dueAt),
  ).length;

  function toggleGroup(key: string) {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

  function toggle(t: Task) {
    onMove(t.id, t.status === "done" ? "todo" : "done", t.position);
  }

  return (
    <div className="mx-auto w-full max-w-[100rem] px-6 pb-8 pt-4 max-sm:px-4">
      <section
        className="mb-5 flex flex-wrap items-center gap-2 rounded-2xl border border-subtle bg-surface p-3 shadow-sm"
        aria-label={strings.taskOvProgress}
      >
        <span className="inline-flex items-center gap-2 rounded-xl bg-raised px-3 py-2 text-sm font-semibold text-primary">
          <ListChecks size={16} className="text-accent" aria-hidden="true" />
          {strings.taskSummaryTotal(ordered.length)}
        </span>
        <span className="rounded-xl px-3 py-2 text-sm font-medium text-secondary">
          {strings.taskSummaryActive(ordered.length - completed)}
        </span>
        <span
          className={`inline-flex items-center gap-1.5 rounded-xl px-3 py-2 text-sm font-medium ${overdue > 0 ? "bg-[var(--danger-tint)] text-danger" : "text-secondary"}`}
        >
          <Clock3 size={15} aria-hidden="true" />
          {strings.taskSummaryOverdue(overdue)}
        </span>
        <span className="rounded-xl px-3 py-2 text-sm font-medium text-secondary">
          {strings.taskSummaryCompleted(completed)}
        </span>
      </section>

      <div className="hidden grid-cols-[minmax(240px,2.4fr)_minmax(120px,1fr)_minmax(120px,1fr)_110px_120px] items-center gap-4 px-5 pb-2 text-xs font-semibold uppercase tracking-wide text-tertiary lg:grid">
        <span>{strings.taskColName}</span>
        <span>{strings.taskColProject}</span>
        <span>{strings.taskColAssignee}</span>
        <span>{strings.taskColDue}</span>
        <span>{strings.taskColPriority}</span>
      </div>

      {groups.map((group) => {
        const isCollapsed = collapsed.has(group.key);
        return (
          <section
            key={group.key}
            className="mb-4 overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm"
          >
            <button
              type="button"
              className="flex min-h-14 w-full items-center gap-3 px-6 py-3 text-left text-secondary transition-colors hover:bg-raised max-sm:px-5"
              onClick={() => toggleGroup(group.key)}
            >
              {isCollapsed ? (
                <ChevronRight size={16} />
              ) : (
                <ChevronDown size={16} />
              )}
              {group.status !== undefined && (
                <span
                  className="size-2.5 shrink-0 rounded-full"
                  style={{ background: statusColor(group.status) }}
                  aria-hidden
                />
              )}
              <span className="text-sm font-semibold text-primary">
                {group.label}
              </span>
              <span className="rounded-full bg-raised px-2 py-0.5 text-xs font-semibold tabular-nums text-tertiary">
                {group.items.length}
              </span>
            </button>

            {!isCollapsed && (
              <>
                {group.items.map((t) => {
                  const done = t.status === "done";
                  return (
                    <div
                      key={t.id}
                      role="button"
                      tabIndex={0}
                      className={`group grid cursor-pointer grid-cols-1 gap-2 border-t border-subtle px-6 transition-colors hover:bg-raised focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent max-sm:px-5 lg:grid-cols-[minmax(240px,2.4fr)_minmax(120px,1fr)_minmax(120px,1fr)_110px_120px] lg:items-center lg:gap-4 ${config.compact ? "py-2" : "py-3.5"} ${done ? "bg-raised/30" : ""}`}
                      onClick={() => onOpen(t.id)}
                      onKeyDown={(event) => {
                        if (event.key === "Enter" || event.key === " ") {
                          event.preventDefault();
                          onOpen(t.id);
                        }
                      }}
                    >
                      <span className="flex min-w-0 items-center gap-3">
                        <button
                          type="button"
                          className={`inline-flex shrink-0 rounded-full text-tertiary transition-colors hover:text-success ${done ? "text-success" : ""}`}
                          onClick={(e) => {
                            e.stopPropagation();
                            toggle(t);
                          }}
                          aria-label={
                            done
                              ? strings.taskMarkNotDone
                              : strings.taskMarkDone
                          }
                        >
                          {done ? (
                            <CheckCircle2 size={18} />
                          ) : (
                            <Circle size={18} />
                          )}
                        </button>
                        <span
                          className={`truncate text-sm font-semibold text-primary transition-colors group-hover:text-accent ${done ? "font-medium text-tertiary line-through group-hover:text-tertiary" : ""}`}
                        >
                          {t.title}
                        </span>
                        <LabelChips labels={t.labels} />
                      </span>
                      <span className="truncate text-xs font-medium text-secondary max-lg:pl-8 lg:text-sm">
                        {projectName(t.projectId)}
                      </span>
                      <span className="truncate text-sm text-secondary max-lg:pl-8">
                        {t.assignee !== null && (
                          <span className="inline-flex min-w-0 items-center gap-2">
                            <Avatar email={t.assignee} />
                            {assigneeName(t.assignee, me)}
                          </span>
                        )}
                      </span>
                      <span
                        className={`truncate text-sm tabular-nums text-secondary max-lg:pl-8 ${
                          t.dueAt !== null && !done && isOverdue(t.dueAt)
                            ? "font-medium text-danger"
                            : ""
                        }`}
                      >
                        {t.dueAt !== null ? dueLabel(t.dueAt) : ""}
                      </span>
                      <span className="truncate text-sm text-secondary max-lg:pl-8">
                        <PriorityCell task={t} />
                      </span>
                    </div>
                  );
                })}
                {onAdd !== undefined && group.status !== undefined && (
                  <button
                    type="button"
                    className="flex min-h-12 w-full items-center gap-2.5 border-t border-subtle px-6 py-3 text-left text-sm font-medium text-tertiary transition-colors hover:bg-[var(--accent-soft)] hover:text-accent max-sm:px-5"
                    onClick={() => onAdd(group.status as string)}
                  >
                    <Plus size={15} /> {strings.taskAdd}
                  </button>
                )}
              </>
            )}
          </section>
        );
      })}
    </div>
  );
}
