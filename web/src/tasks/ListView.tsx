// The list view: a grouped table — the same task rows as the board (ADR 0022),
// here as columns (name · project · assignee · due · priority). The toolbar's
// filter / sort / group config reshapes the loaded tasks (viewConfig); groups
// collapse; status groups get an inline "add task" row. Checking the circle
// moves a task to/from Done via the one-field move the board uses.
import { useRef, useState } from "react";
import {
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Circle,
  Clock3,
  ListChecks,
  Plus,
} from "lucide-react";

import { Badge } from "../ds";
import { strings } from "../i18n";
import type { Task } from "../jmap";
import { Avatar, LabelChips, PriorityChip, dueLabel, isOverdue, statusColor } from "./parts";
import { TaskToolbar } from "./TaskToolbar";
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
  onConfigChange: (next: ViewConfig) => void;
}

function assigneeName(email: string, me?: string): string {
  if (me !== undefined && email.toLowerCase() === me.toLowerCase())
    return strings.taskAssigneeYou;
  const local = email.split("@")[0] ?? email;
  const first = local.split(/[.\-_]+/)[0] ?? local;
  return first.charAt(0).toUpperCase() + first.slice(1);
}

/** The priority column, as `ds/Badge` (D2.11): done wears success with its
 *  tick, and the three priorities map onto danger/warning/neutral — the same
 *  mapping as `parts`' `PriorityChip`, whose hand-drawn twin this was. */
function PriorityCell({ task }: { task: Task }) {
  if (task.status === "done") {
    return (
      <Badge tone="success" className="gap-1.5">
        <CheckCircle2 size={13} aria-hidden="true" />
        {strings.taskColDone}
      </Badge>
    );
  }
  if (task.priority === "none") return <span className="text-tertiary">—</span>;
  return <PriorityChip priority={task.priority} />;
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
  onConfigChange,
}: Props) {
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());
  const [dragId, setDragId] = useState<string | null>(null);
  const [dropGroup, setDropGroup] = useState<string | null>(null);
  const draggedRef = useRef(false);
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

  function dropInto(status: string) {
    if (dragId === null) return;
    const destination = ordered.filter(
      (task) => task.status === status && task.id !== dragId,
    );
    const position = destination.length === 0
      ? 1024
      : Math.max(...destination.map((task) => task.position)) + 1024;
    onMove(dragId, status, position);
    setDragId(null);
    setDropGroup(null);
  }

  return (
    <>
      <TaskToolbar
        config={config}
        onChange={onConfigChange}
        summary={<>
        <span className="inline-flex items-center gap-2 rounded-lg bg-raised px-3 py-2 text-sm font-semibold text-primary">
          <ListChecks size={16} className="text-accent" aria-hidden="true" />
          {strings.taskSummaryTotal(ordered.length)}
        </span>
        <span className="rounded-lg px-3 py-2 text-sm font-medium text-secondary">
          {strings.taskSummaryActive(ordered.length - completed)}
        </span>
        <span
          className={`inline-flex items-center gap-1.5 rounded-lg px-3 py-2 text-sm font-medium ${overdue > 0 ? "bg-danger-tint text-danger" : "text-secondary"}`}
        >
          <Clock3 size={15} aria-hidden="true" />
          {strings.taskSummaryOverdue(overdue)}
        </span>
        <span className="rounded-lg px-3 py-2 text-sm font-medium text-secondary">
          {strings.taskSummaryCompleted(completed)}
        </span>
        </>}
      />

    <div className="mx-auto w-full max-w-[100rem] px-6 pb-8 pt-6 max-sm:px-4">

      <div className="overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm">
        <div className="hidden grid-cols-[minmax(240px,2.4fr)_minmax(120px,1fr)_minmax(120px,1fr)_110px_120px] items-center gap-4 border-b border-subtle bg-raised/60 px-6 py-3 text-[11px] font-semibold uppercase tracking-[0.08em] text-tertiary lg:grid">
          <span>{strings.taskColName}</span>
          <span>{strings.taskColProject}</span>
          <span>{strings.taskColAssignee}</span>
          <span>{strings.taskColDue}</span>
          <span>{strings.taskColPriority}</span>
        </div>

      {groups.map((group, groupIndex) => {
        const isCollapsed = collapsed.has(group.key);
        return (
          <section
            key={group.key}
            className={`transition-[background-color,box-shadow] ${groupIndex > 0 ? "border-t border-subtle" : ""} ${dropGroup === group.key ? "bg-accent-soft shadow-[inset_3px_0_0_var(--accent)]" : "bg-surface"}`}
            onDragOver={(event) => {
              if (dragId === null || group.status === undefined) return;
              event.preventDefault();
              event.dataTransfer.dropEffect = "move";
              setDropGroup(group.key);
            }}
            onDragLeave={(event) => {
              if (!event.currentTarget.contains(event.relatedTarget as Node)) {
                setDropGroup((current) => current === group.key ? null : current);
              }
            }}
            onDrop={(event) => {
              if (group.status === undefined) return;
              event.preventDefault();
              dropInto(group.status);
            }}
          >
            <button
              type="button"
              className="flex min-h-12 w-full items-center gap-3 bg-raised/35 py-2.5 pl-6 pr-6 text-left text-secondary transition-colors hover:bg-raised max-sm:pl-5 max-sm:pr-5"
              onClick={() => toggleGroup(group.key)}
            >
              <span className="grid size-7 shrink-0 place-items-center rounded-lg text-secondary transition-colors group-hover:bg-surface" aria-hidden="true">
                {isCollapsed ? (
                  <ChevronRight size={16} />
                ) : (
                  <ChevronDown size={16} />
                )}
              </span>
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
                      draggable={group.status !== undefined}
                      className={`group grid cursor-grab grid-cols-1 gap-2 border-t border-subtle pl-6 pr-6 transition-[background-color,opacity] hover:bg-raised focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent active:cursor-grabbing max-sm:pl-5 max-sm:pr-5 lg:grid-cols-[minmax(240px,2.4fr)_minmax(120px,1fr)_minmax(120px,1fr)_110px_120px] lg:items-center lg:gap-4 ${config.compact ? "py-2" : "py-3.5"} ${done ? "bg-raised/30" : ""} ${dragId === t.id ? "opacity-45" : ""}`}
                      onClick={() => {
                        if (draggedRef.current) {
                          draggedRef.current = false;
                          return;
                        }
                        onOpen(t.id);
                      }}
                      onDragStart={(event) => {
                        draggedRef.current = true;
                        setDragId(t.id);
                        event.dataTransfer.effectAllowed = "move";
                        event.dataTransfer.setData("text/plain", t.id);
                      }}
                      onDragEnd={() => {
                        setDragId(null);
                        setDropGroup(null);
                        window.setTimeout(() => { draggedRef.current = false; }, 0);
                      }}
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
                          onKeyDown={(event) => event.stopPropagation()}
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
                    className="mx-4 my-2 flex min-h-9 w-[calc(100%-2rem)] items-center gap-2 rounded-lg px-3 py-2 text-left text-sm font-medium text-tertiary transition-colors hover:bg-accent-soft hover:text-accent"
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
    </div>
    </>
  );
}
