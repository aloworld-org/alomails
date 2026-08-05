// Timeline: a Gantt-lite over real task dates — each task a bar from when it was
// created to its due date, on a horizontal day axis, coloured by workflow status
// (the shared status palette). Tasks with no due date are listed separately.
// Dependency edges (task X blocked by task Y) are drawn as arrows from the
// blocker's bar to the blocked task's bar, when both are scheduled.
import { useMemo } from "react";

import { getLocale, strings } from "../i18n";
import type { Task, TaskDepEdgeDto } from "../jmap";
import { addDays, startOfDay } from "../agenda/dates";
import { Avatar, COLUMNS, columnLabel, statusColor } from "./parts";
import styles from "./TasksModule.module.css";

const DAY = 86400000;
const COL = 40; // px per day
const LABEL = 240; // px, the sticky name column
const ROW = 40; // px, one task row's height (matches .tlRow)

interface Props {
  tasks: Task[];
  edges?: TaskDepEdgeDto[];
  onOpen: (id: string) => void;
}

export function TimelineView({ tasks, edges = [], onOpen }: Props) {
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

  // Bar geometry per scheduled task, in the coordinate space of `.tlRows`
  // (x measured from the sticky label column, y down the rows) — the anchors the
  // dependency arrows connect.
  const geom = new Map<string, { x1: number; x2: number; y: number }>();
  scheduled.forEach((t, i) => {
    const { left, width } = bar(t);
    geom.set(t.id, { x1: LABEL + left, x2: LABEL + left + width, y: i * ROW + 20 });
  });
  const arrows = edges
    .map((e) => ({ blocked: geom.get(e.blocked), blocker: geom.get(e.blockedBy) }))
    .filter((a): a is { blocked: { x1: number; x2: number; y: number }; blocker: { x1: number; x2: number; y: number } } => a.blocked !== undefined && a.blocker !== undefined);

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
        {arrows.length > 0 && (
          <svg
            className={styles.tlArrows}
            width={LABEL + total}
            height={scheduled.length * ROW}
            aria-hidden
          >
            <defs>
              <marker
                id="tl-arrowhead"
                viewBox="0 0 10 10"
                refX="8"
                refY="5"
                markerWidth="6"
                markerHeight="6"
                orient="auto-start-reverse"
              >
                <path d="M0,0 L10,5 L0,10 z" fill="var(--text-tertiary)" />
              </marker>
            </defs>
            {arrows.map((a, i) => {
              // From the blocker's right end to the blocked task's left start:
              // out a little, then an elbow to the target row.
              const sx = a.blocker.x2;
              const sy = a.blocker.y;
              const tx = a.blocked.x1;
              const ty = a.blocked.y;
              const midX = Math.max(sx + 12, tx - 12);
              const d = `M ${sx} ${sy} H ${midX} V ${ty} H ${tx}`;
              return (
                <path
                  key={i}
                  d={d}
                  className={styles.tlArrowPath}
                  markerEnd="url(#tl-arrowhead)"
                />
              );
            })}
          </svg>
        )}
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
