// The list: the same task rows as the board (ADR 0022), flat and ordered.
// Clean rows — a completion toggle, the title, then the source mark, priority,
// due date and assignee. Checking the circle moves the task to (or out of) the
// Done column via the same one-field move the board uses.
import { CheckCircle2, Circle } from "lucide-react";

import type { Task } from "../jmap";
import { Avatar, DueChip, PriorityChip, SourceMark } from "./parts";
import styles from "./TasksModule.module.css";

interface Props {
  tasks: Task[];
  onOpen: (id: string) => void;
  onMove: (id: string, status: string, position: number) => void;
}

export function ListView({ tasks, onOpen, onMove }: Props) {
  const ordered = [...tasks].sort((a, b) => {
    const order = (s: string) => (s === "done" ? 2 : s === "in_progress" ? 1 : 0);
    return order(a.status) - order(b.status) || a.position - b.position;
  });

  function toggle(t: Task) {
    const status = t.status === "done" ? "todo" : "done";
    onMove(t.id, status, t.position);
  }

  return (
    <div className={styles.list}>
      {ordered.map((t) => {
        const done = t.status === "done";
        return (
          <div
            key={t.id}
            className={`${styles.row} ${done ? styles.rowDone : ""}`}
            onClick={() => onOpen(t.id)}
          >
            <button
              type="button"
              className={styles.check}
              onClick={(e) => {
                e.stopPropagation();
                toggle(t);
              }}
              aria-label={done ? "Mark not done" : "Mark done"}
            >
              {done ? <CheckCircle2 size={18} /> : <Circle size={18} />}
            </button>
            <span className={styles.rowTitle}>{t.title}</span>
            <div className={styles.rowMeta}>
              <SourceMark task={t} />
              <PriorityChip priority={t.priority} />
              {t.dueAt && <DueChip iso={t.dueAt} done={done} />}
              {t.assignee && <Avatar email={t.assignee} />}
            </div>
          </div>
        );
      })}
    </div>
  );
}
