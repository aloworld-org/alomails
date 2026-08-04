// The list view: a grouped table — the same task rows as the board (ADR 0022),
// here as columns (name · project · assignee · due · priority). The toolbar's
// filter / sort / group config reshapes the loaded tasks (viewConfig); groups
// collapse; status groups get an inline "add task" row. Checking the circle
// moves a task to/from Done via the one-field move the board uses.
import { useState } from "react";
import { CheckCircle2, ChevronDown, ChevronRight, Circle, Plus } from "lucide-react";

import { strings } from "../i18n";
import type { Task } from "../jmap";
import { Avatar, dueLabel, isOverdue } from "./parts";
import { filterTasks, groupTasks, sortTasks, type ViewConfig } from "./viewConfig";
import styles from "./TasksModule.module.css";

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
      <span className={styles.prioCell}>
        <span className={`${styles.prioDot} ${styles.prioDotDone}`} aria-hidden />
        {strings.taskColDone}
      </span>
    );
  }
  if (task.priority === "none") return <span className={styles.prioCellEmpty}>—</span>;
  const cls =
    task.priority === "high"
      ? styles.prioDotHigh
      : task.priority === "medium"
        ? styles.prioDotMedium
        : styles.prioDotLow;
  const label =
    task.priority === "high"
      ? strings.taskPrioHigh
      : task.priority === "medium"
        ? strings.taskPrioMedium
        : strings.taskPrioLow;
  return (
    <span className={styles.prioCell}>
      <span className={`${styles.prioDot} ${cls}`} aria-hidden />
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
    <div className={`${styles.table} ${config.compact ? styles.tableCompact : ""}`}>
      <div className={styles.tHead}>
        <span>{strings.taskColName}</span>
        <span>{strings.taskColProject}</span>
        <span>{strings.taskColAssignee}</span>
        <span>{strings.taskColDue}</span>
        <span>{strings.taskColPriority}</span>
      </div>

      {groups.map((group) => {
        const isCollapsed = collapsed.has(group.key);
        return (
          <div key={group.key} className={styles.tGroup}>
            <button type="button" className={styles.tGroupHead} onClick={() => toggleGroup(group.key)}>
              {isCollapsed ? <ChevronRight size={16} /> : <ChevronDown size={16} />}
              <span className={styles.tGroupName}>{group.label}</span>
              <span className={styles.tGroupCount}>{group.items.length}</span>
            </button>

            {!isCollapsed && (
              <>
                {group.items.map((t) => {
                  const done = t.status === "done";
                  return (
                    <div
                      key={t.id}
                      className={`${styles.tRow} ${done ? styles.tRowDone : ""}`}
                      onClick={() => onOpen(t.id)}
                    >
                      <span className={styles.tName}>
                        <button
                          type="button"
                          className={styles.tCheck}
                          onClick={(e) => {
                            e.stopPropagation();
                            toggle(t);
                          }}
                          aria-label={done ? strings.taskMarkNotDone : strings.taskMarkDone}
                        >
                          {done ? <CheckCircle2 size={18} /> : <Circle size={18} />}
                        </button>
                        <span className={styles.tTitle}>{t.title}</span>
                      </span>
                      <span className={styles.tCell}>{projectName(t.projectId)}</span>
                      <span className={styles.tCell}>
                        {t.assignee !== null && (
                          <span className={styles.tAssignee}>
                            <Avatar email={t.assignee} />
                            {assigneeName(t.assignee, me)}
                          </span>
                        )}
                      </span>
                      <span
                        className={`${styles.tCell} ${
                          t.dueAt !== null && !done && isOverdue(t.dueAt) ? styles.tDueOverdue : ""
                        }`}
                      >
                        {t.dueAt !== null ? dueLabel(t.dueAt) : ""}
                      </span>
                      <span className={styles.tCell}>
                        <PriorityCell task={t} />
                      </span>
                    </div>
                  );
                })}
                {onAdd !== undefined && group.status !== undefined && (
                  <button
                    type="button"
                    className={styles.tAddRow}
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
