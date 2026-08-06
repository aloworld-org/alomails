// alo Base views (ADR 0032) — board / gallery / calendar over the SAME records.
// Switching view never changes data; a view only groups or lays out records. The
// board groups by a select field (drag a card to change it); the calendar places
// records by a date field; the gallery is a card wall.
import { useMemo, useState } from "react";
import { ChevronLeft, ChevronRight, Plus } from "lucide-react";

import { strings } from "../i18n";
import type { BaseFieldDto, BaseTableDto } from "../jmap";
import { addDays, startOfDay } from "../agenda/dates";
import { CellDisplay, recordLabel } from "./BaseCell";
import styles from "./BaseEditor.module.css";

interface ViewProps {
  table: BaseTableDto;
  tables: BaseTableDto[];
  config: Record<string, unknown>;
  onSetCell: (recordId: string, fieldId: string, value: unknown) => void;
  onAddRow: (preset?: Record<string, unknown>) => void;
}

function secondaryFields(table: BaseTableDto): BaseFieldDto[] {
  return table.fields.slice(1, 5);
}

// ---- board ------------------------------------------------------------------

export function BoardView({ table, tables, config, onSetCell, onAddRow }: ViewProps) {
  const groupId = typeof config.groupFieldId === "string" ? config.groupFieldId : "";
  const field = table.fields.find((f) => f.id === groupId);
  if (!field || (field.type !== "select" && field.type !== "multiselect")) {
    return <div className={styles.viewMsg}>{strings.baseBoardNeedsSelect}</div>;
  }
  const choices = Array.isArray(field.options.choices)
    ? field.options.choices.filter((c): c is string => typeof c === "string")
    : [];
  const columns = [...choices, ""]; // trailing "" = the uncategorised column

  return (
    <div className={styles.board}>
      {columns.map((choice) => {
        const records = table.records.filter((r) => {
          const v = r.cells[field.id];
          return choice === "" ? v === undefined || v === null || v === "" : v === choice;
        });
        return (
          <div
            key={choice || "__none"}
            className={styles.boardCol}
            onDragOver={(e) => e.preventDefault()}
            onDrop={(e) => {
              const id = e.dataTransfer.getData("text/plain");
              if (id) onSetCell(id, field.id, choice === "" ? null : choice);
            }}
          >
            <div className={styles.boardColHead}>
              {choice === "" ? strings.baseUncategorised : choice}
              <span className={styles.boardCount}>{records.length}</span>
            </div>
            {records.map((r) => (
              <div
                key={r.id}
                className={styles.boardCard}
                draggable
                onDragStart={(e) => e.dataTransfer.setData("text/plain", r.id)}
              >
                <div className={styles.boardCardTitle}>{recordLabel(table, r)}</div>
                {secondaryFields(table).map((f) => {
                  const node = <CellDisplay field={f} value={r.cells[f.id]} tables={tables} />;
                  return node ? (
                    <div key={f.id} className={styles.boardCardField}>
                      {node}
                    </div>
                  ) : null;
                })}
              </div>
            ))}
            <button
              type="button"
              className={styles.boardAdd}
              onClick={() => onAddRow(choice === "" ? {} : { [field.id]: choice })}
            >
              <Plus size={14} /> {strings.baseNewRow}
            </button>
          </div>
        );
      })}
    </div>
  );
}

// ---- gallery ----------------------------------------------------------------

export function GalleryView({ table, tables, onAddRow }: ViewProps) {
  return (
    <div className={styles.gallery}>
      {table.records.map((r) => (
        <div key={r.id} className={styles.galleryCard}>
          <div className={styles.galleryTitle}>{recordLabel(table, r)}</div>
          {secondaryFields(table).map((f) => {
            const node = <CellDisplay field={f} value={r.cells[f.id]} tables={tables} />;
            return node ? (
              <div key={f.id} className={styles.galleryField}>
                <span className={styles.galleryFieldName}>{f.name}</span>
                <span>{node}</span>
              </div>
            ) : null;
          })}
        </div>
      ))}
      <button type="button" className={styles.galleryAdd} onClick={() => onAddRow()}>
        <Plus size={16} /> {strings.baseNewRow}
      </button>
    </div>
  );
}

// ---- calendar ---------------------------------------------------------------

export function CalendarView({ table, config }: ViewProps) {
  const dateId = typeof config.dateFieldId === "string" ? config.dateFieldId : "";
  const field = table.fields.find((f) => f.id === dateId);
  const [monthStart, setMonthStart] = useState(() => {
    const now = startOfDay(new Date());
    return new Date(now.getFullYear(), now.getMonth(), 1);
  });

  const grid = useMemo(() => {
    // Monday-first grid covering the month.
    const first = monthStart;
    const offset = (first.getDay() + 6) % 7;
    const start = addDays(first, -offset);
    return Array.from({ length: 42 }, (_, i) => addDays(start, i));
  }, [monthStart]);

  if (!field || field.type !== "date") {
    return <div className={styles.viewMsg}>{strings.baseCalendarNeedsDate}</div>;
  }

  const byDay = new Map<string, { id: string; label: string }[]>();
  for (const r of table.records) {
    const v = r.cells[field.id];
    if (typeof v === "string" && v !== "") {
      const key = v.slice(0, 10);
      const list = byDay.get(key) ?? [];
      list.push({ id: r.id, label: recordLabel(table, r) });
      byDay.set(key, list);
    }
  }
  const monthLabel = new Intl.DateTimeFormat(undefined, { month: "long", year: "numeric" }).format(monthStart);
  const iso = (d: Date) => new Date(d.getTime() - d.getTimezoneOffset() * 60000).toISOString().slice(0, 10);

  return (
    <div className={styles.calWrap}>
      <div className={styles.calNav}>
        <button type="button" onClick={() => setMonthStart(new Date(monthStart.getFullYear(), monthStart.getMonth() - 1, 1))} aria-label="prev">
          <ChevronLeft size={16} />
        </button>
        <span className={styles.calMonth}>{monthLabel}</span>
        <button type="button" onClick={() => setMonthStart(new Date(monthStart.getFullYear(), monthStart.getMonth() + 1, 1))} aria-label="next">
          <ChevronRight size={16} />
        </button>
      </div>
      <div className={styles.calGrid}>
        {grid.map((d, i) => {
          const inMonth = d.getMonth() === monthStart.getMonth();
          const recs = byDay.get(iso(d)) ?? [];
          return (
            <div key={i} className={inMonth ? styles.calDay : `${styles.calDay} ${styles.calDayOut}`}>
              <span className={styles.calDayNum}>{d.getDate()}</span>
              {recs.map((r) => (
                <span key={r.id} className={styles.calChip}>
                  {r.label}
                </span>
              ))}
            </div>
          );
        })}
      </div>
    </div>
  );
}
