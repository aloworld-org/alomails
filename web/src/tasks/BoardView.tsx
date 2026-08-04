// The board (kanban): the project's tasks grouped into status columns, the same
// rows the list renders (ADR 0022). Drag a card between columns to change its
// status, or drop it onto a card to reorder — each is a single-field move whose
// position is a fractional index (midpoint of its new neighbours), so one row
// changes. Native HTML5 drag-and-drop; no dependency.
import { useState } from "react";
import { MessageSquare } from "lucide-react";

import type { Task } from "../jmap";
import { Avatar, COLUMNS, DueChip, PriorityChip, SourceMark, columnLabel } from "./parts";
import styles from "./TasksModule.module.css";

interface Props {
  tasks: Task[];
  onOpen: (id: string) => void;
  onMove: (id: string, status: string, position: number) => void;
}

export function BoardView({ tasks, onOpen, onMove }: Props) {
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
              {columnLabel(c.key)} <span className={styles.columnCount}>{cards.length}</span>
            </div>
            {cards.map((t) => (
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
                  <PriorityChip priority={t.priority} />
                  {t.dueAt && <DueChip iso={t.dueAt} done={t.status === "done"} />}
                  <SourceMark task={t} />
                  {t.subtaskTotal > 0 && (
                    <span className={styles.metaIcon}>
                      ✓ {t.subtaskDone}/{t.subtaskTotal}
                    </span>
                  )}
                  {t.commentCount > 0 && (
                    <span className={styles.metaIcon}>
                      <MessageSquare size={12} /> {t.commentCount}
                    </span>
                  )}
                  <span style={{ marginLeft: "auto" }}>
                    {t.assignee && <Avatar email={t.assignee} />}
                  </span>
                </div>
              </div>
            ))}
          </div>
        );
      })}
    </div>
  );
}
