// Timeline: a Gantt-lite over real task dates — each task a bar from when it was
// created to its due date, on a horizontal day axis, coloured by workflow status
// (the shared status palette). Tasks with no due date are listed separately.
// (Dependency arrows would need a task-dependency model we don't have yet.)
import { useMemo } from "react";

import { getLocale, strings } from "../i18n";
import type { Task } from "../jmap";
import { addDays, startOfDay } from "../agenda/dates";
import { Avatar, COLUMNS, columnLabel, statusColor } from "./parts";
import styles from "./TasksModule.module.css";

const DAY = 86400000;
const COL = 40; // px per day
const LABEL = 240; // px, the sticky name column

interface Props {
  tasks: Task[];
  onOpen: (id: string) => void;
}

export function TimelineView({ tasks, onOpen }: Props) {
  const locale = getLocale();
  const scheduled = tasks.filter((t) => t.dueAt !== null);
  const unscheduled = tasks.filter((t) => t.dueAt === null);

  const { from, days } = useMemo(() => {
    const today = startOfDay(new Date());
    let min = addDays(today, -3).getTime();
    let max = addDays(today, 21).getTime();
    for (const t of scheduled) {
      const due = new Date(t.dueAt as string).getTime();
      const created = new Date(t.createdAt).getTime();
      min = Math.min(min, created, due);
      max = Math.max(max, due);
    }
    const fromD = startOfDay(new Date(min));
    const count = Math.min(Math.ceil((max - fromD.getTime()) / DAY) + 2, 160);
    return { from: fromD, days: count };
  }, [scheduled]);

  const total = days * COL;
  const today = startOfDay(new Date());
  const dayFmt = new Intl.DateTimeFormat(locale, { day: "numeric" });
  const weekdayFmt = new Intl.DateTimeFormat(locale, { weekday: "narrow" });
  const rangeFmt = new Intl.DateTimeFormat(locale, { month: "long", year: "numeric" });
  const rangeLabel = (() => {
    const last = addDays(from, days - 1);
    const a = new Intl.DateTimeFormat(locale, { month: "long" }).format(from);
    const b = rangeFmt.format(last);
    return from.getMonth() === last.getMonth() ? b : `${a} – ${b}`;
  })();

  function bar(t: Task) {
    const due = new Date(t.dueAt as string).getTime();
    const created = new Date(t.createdAt).getTime();
    const startMs = startOfDay(new Date(Math.min(created, due))).getTime();
    const endMs = startOfDay(new Date(due)).getTime() + DAY; // inclusive of the due day
    const left = ((startMs - from.getTime()) / DAY) * COL;
    const width = Math.max(COL - 6, ((endMs - startMs) / DAY) * COL - 4);
    return { left, width };
  }

  return (
    <div className={styles.timeline}>
      <div className={styles.tlTitle}>{rangeLabel}</div>

      <div className={styles.tlHeadRow}>
        <div className={styles.tlCorner} style={{ width: LABEL }}>
          {strings.taskColName}
        </div>
        <div className={styles.tlDays} style={{ width: total }}>
          {Array.from({ length: days }, (_, i) => {
            const d = addDays(from, i);
            const isMonthStart = d.getDate() === 1 || i === 0;
            const isToday = d.getTime() === today.getTime();
            return (
              <div key={i} className={`${styles.tlDay} ${isToday ? styles.tlDayToday : ""}`} style={{ width: COL }}>
                {isMonthStart && <span className={styles.tlMonth}>{new Intl.DateTimeFormat(locale, { month: "short" }).format(d)}</span>}
                <span className={styles.tlWeekday}>{weekdayFmt.format(d)}</span>
                <span>{dayFmt.format(d)}</span>
              </div>
            );
          })}
        </div>
      </div>

      <div className={styles.tlRows}>
        {scheduled.map((t) => {
          const { left, width } = bar(t);
          return (
            <div key={t.id} className={styles.tlRow}>
              <div className={styles.tlLabel} style={{ width: LABEL }} title={t.title}>
                {t.assignee !== null && <Avatar email={t.assignee} />}
                <span className={styles.tlLabelText}>{t.title}</span>
              </div>
              <div className={styles.tlTrack} style={{ width: total }}>
                <button
                  type="button"
                  className={styles.tlBar}
                  style={{ left, width, background: statusColor(t.status) }}
                  onClick={() => onOpen(t.id)}
                  title={t.title}
                >
                  <span className={styles.tlBarText}>{columnLabel(t.status)}</span>
                </button>
              </div>
            </div>
          );
        })}
      </div>

      <div className={styles.tlLegend}>
        {COLUMNS.map((c) => (
          <span key={c.key} className={styles.tlLegendItem}>
            <span className={styles.tlLegendDot} style={{ background: statusColor(c.key) }} aria-hidden />
            {c.label()}
          </span>
        ))}
      </div>

      {unscheduled.length > 0 && (
        <div className={styles.tlUnscheduled}>
          <div className={styles.tlUnschedHead}>{strings.taskUnscheduled}</div>
          {unscheduled.map((t) => (
            <button key={t.id} type="button" className={styles.tlUnschedItem} onClick={() => onOpen(t.id)}>
              {t.title}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
