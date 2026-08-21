// The board (kanban): the project's tasks grouped into status columns, the same
// rows the list renders (ADR 0022). Drag a card between columns to change its
// status, or drop it onto a card to reorder — each is a single-field move whose
// position is a fractional index (midpoint of its new neighbours), so one row
// changes. Native HTML5 drag-and-drop.
import { useState } from "react";
import { CalendarDays, Plus } from "lucide-react";

import { strings } from "../i18n";
import type { Task } from "../jmap";
import { Avatar, COLUMNS, LabelChips, columnLabel, dueLabel, isOverdue, statusColor } from "./parts";

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
    <div className="flex min-h-full items-start gap-4 overflow-x-auto p-4">
      {COLUMNS.map((c) => {
        const cards = inColumn(c.key);
        const color = statusColor(c.key);
        return (
          <div
            key={c.key}
            className={`flex w-72 shrink-0 flex-col gap-2 rounded-xl bg-app p-2.5 ${
              overCol === c.key ? "outline-2 outline-dashed -outline-offset-2 outline-accent" : ""
            }`}
            onDragOver={(e) => {
              e.preventDefault();
              setOverCol(c.key);
            }}
            onDragLeave={() => setOverCol((s) => (s === c.key ? null : s))}
            onDrop={() => dropOnColumn(c.key)}
          >
            <div className="flex items-center gap-2 px-1 py-0.5 text-sm font-semibold text-secondary">
              <span className="size-2 shrink-0 rounded-full" style={{ background: color }} aria-hidden />
              <span className="text-primary">{columnLabel(c.key)}</span>
              <span className="font-normal text-tertiary">{cards.length}</span>
            </div>
            {cards.map((t) => {
              const done = t.status === "done";
              return (
                <div
                  key={t.id}
                  className={`flex cursor-pointer flex-col gap-2 rounded-lg border border-subtle bg-surface px-3 py-2.5 shadow-sm transition-[border-color,box-shadow,opacity] hover:border-default hover:shadow-md ${
                    dragId === t.id ? "opacity-40" : ""
                  }`}
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
                  <div className="text-sm leading-snug text-primary">{t.title}</div>
                  <LabelChips labels={t.labels} />
                  <div className="flex flex-wrap items-center gap-2">
                    {t.dueAt !== null && (
                      <span
                        className="inline-flex items-center gap-1 text-xs font-medium tabular-nums"
                        style={{ color: !done && isOverdue(t.dueAt) ? "var(--danger)" : color }}
                      >
                        <CalendarDays size={13} />
                        {dueLabel(t.dueAt)}
                      </span>
                    )}
                    <span className="flex-1" />
                    {t.assignee !== null && <Avatar email={t.assignee} />}
                  </div>
                </div>
              );
            })}
            {onAdd !== undefined && (
              <button type="button" className="flex w-full items-center gap-1.5 rounded-lg p-2 text-sm text-tertiary transition-colors hover:bg-surface hover:text-accent" onClick={() => onAdd(c.key)}>
                <Plus size={15} /> {strings.taskAdd}
              </button>
            )}
          </div>
        );
      })}
    </div>
  );
}
