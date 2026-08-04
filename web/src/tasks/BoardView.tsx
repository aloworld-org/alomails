// The board (kanban): the project's tasks grouped into status columns, the same
// rows the list renders (ADR 0022). Drag a card between columns to change its
// status, or drop it onto a card to reorder — each is a single-field move whose
// position is a fractional index (midpoint of its new neighbours), so one row
// changes. Native HTML5 drag-and-drop.
import { useState } from "react";
import { CalendarDays, Plus } from "lucide-react";

import { strings } from "../i18n";
import type { Task } from "../jmap";
import { Avatar, COLUMNS, columnLabel, dueLabel, isOverdue, statusColor } from "./parts";
import styles from "./TasksModule.module.css";

interface Props {
  tasks: Task[];
  onOpen: (id: string) => void;
  onMove: (id: string, status: string, position: number) => void;
  onAdd?: (status: string) => void;
}

export function BoardView({ tasks, onOpen, onMove, onAdd }: Props) {
  const [dragId, setDragId] = useState<string | null>(null);
  const [overCol, setOverCol] = useState<string | null>(null);

  const inColumn = (status: string) =>
    tasks.filter((t) => t.status === status).sort((a, b) => a.position - b.position);

  function dropOnColumn(status: string) {
    if (dragId) {
      const col = inColumn(status).filter((t) => t.id !== dragId);
      const position = (col.at(-1)?.position ?? 0) + 1;
      onMove(dragId, status, position);
    }
    setDragId(null);
    setOverCol(null);
  }

  function dropOnCard(status: string, targetId: string) {
    if (dragId && dragId !== targetId) {
      const col = inColumn(status).filter((t) => t.id !== dragId);
      const target = col.find((t) => t.id === targetId);
      if (target) {
        const idx = col.indexOf(target);
        const prev = col[idx - 1]?.position ?? target.position - 1;
        onMove(dragId, status, (prev + target.position) / 2);
      }
    }
    setDragId(null);
    setOverCol(null);
  }

  return (
    <div className={styles.board}>
      {COLUMNS.map((c) => {
        const cards = inColumn(c.key);
        const color = statusColor(c.key);
        return (
          <div
            key={c.key}
            className={`${styles.column} ${overCol === c.key ? styles.columnOver : ""}`}
            onDragOver={(e) => {
              e.preventDefault();
              setOverCol(c.key);
            }}
            onDragLeave={() => setOverCol((s) => (s === c.key ? null : s))}
            onDrop={() => dropOnColumn(c.key)}
          >
            <div className={styles.columnHead}>
              <span className={styles.columnDot} style={{ background: color }} aria-hidden />
              <span className={styles.columnName}>{columnLabel(c.key)}</span>
              <span className={styles.columnCount}>{cards.length}</span>
            </div>
            {cards.map((t) => {
              const done = t.status === "done";
              return (
                <div
                  key={t.id}
                  className={`${styles.card} ${dragId === t.id ? styles.cardDragging : ""}`}
                  draggable
                  onDragStart={() => setDragId(t.id)}
                  onDragEnd={() => {
                    setDragId(null);
                    setOverCol(null);
                  }}
                  onDrop={(e) => {
                    e.stopPropagation();
                    dropOnCard(c.key, t.id);
                  }}
                  onClick={() => onOpen(t.id)}
                >
                  <div className={styles.cardTitle}>{t.title}</div>
                  <div className={styles.cardMeta}>
                    {t.dueAt !== null && (
                      <span
                        className={styles.cardDue}
                        style={{ color: !done && isOverdue(t.dueAt) ? "var(--danger)" : color }}
                      >
                        <CalendarDays size={13} />
                        {dueLabel(t.dueAt)}
                      </span>
                    )}
                    <span className={styles.cardSpacer} />
                    {t.assignee !== null && <Avatar email={t.assignee} />}
                  </div>
                </div>
              );
            })}
            {onAdd !== undefined && (
              <button type="button" className={styles.cardAdd} onClick={() => onAdd(c.key)}>
                <Plus size={15} /> {strings.taskAdd}
              </button>
            )}
          </div>
        );
      })}
    </div>
  );
}
