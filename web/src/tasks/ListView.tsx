// The list view: a grouped table — the same task rows as the board (ADR 0022),
// here as columns (name · project · assignee · due · priority). The toolbar's
// filter / sort / group config reshapes the loaded tasks (viewConfig); groups
// collapse; status groups get an inline "add task" row. Checking the circle
// moves a task to/from Done via the one-field move the board uses.
import { useState } from "react";
import { CheckCircle2, ChevronDown, ChevronRight, Circle, Plus } from "lucide-react";

import { strings } from "../i18n";
import type { Task } from "../jmap";
import { Avatar, LabelChips, dueLabel, isOverdue, statusColor } from "./parts";
import { filterTasks, groupTasks, sortTasks, type ViewConfig } from "./viewConfig";

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
  if (me !== undefined && email.toLowerCase() === me.toLowerCase()) return strings.taskAssigneeYou;
  const local = email.split("@")[0] ?? email;
  const first = local.split(/[.\-_]+/)[0] ?? local;
  return first.charAt(0).toUpperCase() + first.slice(1);
}

function PriorityCell({ task }: { task: Task }) {
  if (task.status === "done") {
    return (
      <span className="inline-flex items-center gap-2 text-sm text-primary">
        <span className="size-2 shrink-0 rounded-full bg-success" aria-hidden />
        {strings.taskColDone}
      </span>
    );
  }
  if (task.priority === "none") return <span className="text-tertiary">—</span>;
  const dotClass =
    task.priority === "high"
      ? "bg-danger"
      : task.priority === "medium"
        ? "bg-warning"
        : "bg-success";
  const label =
    task.priority === "high"
      ? strings.taskPrioHigh
      : task.priority === "medium"
        ? strings.taskPrioMedium
        : strings.taskPrioLow;
  return (
    <span className="inline-flex items-center gap-2 text-sm text-primary">
      <span className={`size-2 shrink-0 rounded-full ${dotClass}`} aria-hidden />
      {label}
    </span>
  );
}

export function ListView({ tasks, config, projectName, me, search, onOpen, onMove, onAdd }: Props) {
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
    <div className="px-6 pb-6 pt-3">
      <div className="grid grid-cols-[minmax(220px,2.4fr)_minmax(110px,1fr)_minmax(110px,1fr)_100px_120px] items-center gap-3 border-b border-subtle px-3 py-2 text-sm font-medium text-tertiary">
        <span>{strings.taskColName}</span>
        <span>{strings.taskColProject}</span>
        <span>{strings.taskColAssignee}</span>
        <span>{strings.taskColDue}</span>
        <span>{strings.taskColPriority}</span>
      </div>

      {groups.map((group) => {
        const isCollapsed = collapsed.has(group.key);
        return (
          <div key={group.key} className="mt-2">
            <button type="button" className="flex w-full items-center gap-2 rounded-lg px-3 pb-2 pt-3 text-left text-secondary transition-colors hover:bg-raised" onClick={() => toggleGroup(group.key)}>
              {isCollapsed ? <ChevronRight size={16} /> : <ChevronDown size={16} />}
              {group.status !== undefined && (
                <span className="size-2.5 shrink-0 rounded-full" style={{ background: statusColor(group.status) }} aria-hidden />
              )}
              <span className="text-base font-bold text-primary">{group.label}</span>
              <span className="text-sm tabular-nums text-tertiary">{group.items.length}</span>
            </button>

            {!isCollapsed && (
              <>
                {group.items.map((t) => {
                  const done = t.status === "done";
                  return (
                    <div
                      key={t.id}
                      className={`grid cursor-pointer grid-cols-[minmax(220px,2.4fr)_minmax(110px,1fr)_minmax(110px,1fr)_100px_120px] items-center gap-3 border-t border-subtle px-3 transition-colors hover:bg-raised ${config.compact ? "py-[5px]" : "py-3"}`}
                      onClick={() => onOpen(t.id)}
                    >
                      <span className="flex min-w-0 items-center gap-3">
                        <button
                          type="button"
                          className={`inline-flex shrink-0 rounded-full text-tertiary transition-colors hover:text-success ${done ? "text-success" : ""}`}
                          onClick={(e) => {
                            e.stopPropagation();
                            toggle(t);
                          }}
                          aria-label={done ? strings.taskMarkNotDone : strings.taskMarkDone}
                        >
                          {done ? <CheckCircle2 size={18} /> : <Circle size={18} />}
                        </button>
                        <span className={`truncate text-base text-primary ${done ? "text-tertiary line-through" : ""}`}>{t.title}</span>
                        <LabelChips labels={t.labels} />
                      </span>
                      <span className="truncate text-sm tabular-nums text-secondary">{projectName(t.projectId)}</span>
                      <span className="truncate text-sm tabular-nums text-secondary">
                        {t.assignee !== null && (
                          <span className="inline-flex min-w-0 items-center gap-2">
                            <Avatar email={t.assignee} />
                            {assigneeName(t.assignee, me)}
                          </span>
                        )}
                      </span>
                      <span
                        className={`truncate text-sm tabular-nums text-secondary ${
                          t.dueAt !== null && !done && isOverdue(t.dueAt) ? "font-medium text-danger" : ""
                        }`}
                      >
                        {t.dueAt !== null ? dueLabel(t.dueAt) : ""}
                      </span>
                      <span className="truncate text-sm tabular-nums text-secondary">
                        <PriorityCell task={t} />
                      </span>
                    </div>
                  );
                })}
                {onAdd !== undefined && group.status !== undefined && (
                  <button
                    type="button"
                    className="flex w-full items-center gap-2 border-t border-subtle px-3 py-3 text-left text-sm text-tertiary transition-colors hover:bg-raised hover:text-accent"
                    onClick={() => onAdd(group.status as string)}
                  >
                    <Plus size={15} /> {strings.taskAdd}
                  </button>
                )}
              </>
            )}
          </div>
        );
      })}
    </div>
  );
}
