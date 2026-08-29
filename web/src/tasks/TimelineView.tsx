// Timeline: a Gantt-lite over real task dates — each task a bar from when it was
// created to its due date, on a horizontal day axis, coloured by workflow status
// (the shared status palette). Tasks with no due date are listed separately.
// Dependency edges (task X blocked by task Y) are drawn as arrows from the
// blocker's bar to the blocked task's bar, when both are scheduled.
import { useMemo } from "react";
import { CalendarDays, Clock3 } from "lucide-react";

import { getLocale, strings } from "../i18n";
import type { Task, TaskDepEdgeDto } from "../jmap";
import { addDays, startOfDay } from "../agenda/dates";
import { Avatar, COLUMNS, columnLabel, dueLabel, statusColor } from "./parts";

const DAY = 86400000;
const COL = 44; // px per day
const LABEL = 280; // px, the sticky name column
const ROW = 52; // px, one task row's height

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
    geom.set(t.id, { x1: LABEL + left, x2: LABEL + left + width, y: i * ROW + ROW / 2 });
  });
  const arrows = edges
    .map((e) => ({ blocked: geom.get(e.blocked), blocker: geom.get(e.blockedBy) }))
    .filter((a): a is { blocked: { x1: number; x2: number; y: number }; blocker: { x1: number; x2: number; y: number } } => a.blocked !== undefined && a.blocker !== undefined);

  const dates = Array.from({ length: days }, (_, i) => addDays(from, i));

  return (
    <div className="mx-auto w-full max-w-[100rem] px-6 pb-8 pt-6 max-sm:px-4">
      <section className="overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm">
        <header className="flex flex-wrap items-center justify-between gap-4 border-b border-subtle px-5 py-4">
          <div className="flex items-center gap-3">
            <span className="grid size-10 place-items-center rounded-xl bg-accent-soft text-accent" aria-hidden="true">
              <CalendarDays size={19} />
            </span>
            <div>
              <h2 className="m-0 text-base font-semibold text-primary">{rangeLabel}</h2>
              <p className="m-0 mt-0.5 text-xs text-tertiary">{strings.taskSummaryTotal(tasks.length)}</p>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-4" aria-label={strings.taskOvProgress}>
            {COLUMNS.map((c) => (
              <span key={c.key} className="inline-flex items-center gap-1.5 text-xs font-medium text-secondary">
                <span className="size-2 rounded-full" style={{ background: statusColor(c.key) }} aria-hidden />
                {c.label()}
              </span>
            ))}
          </div>
        </header>

        <div className="overflow-auto [scrollbar-color:var(--border-default)_transparent] [scrollbar-width:thin]">
          <div style={{ minWidth: LABEL + total }}>
            <div className="sticky top-0 z-30 flex h-14 bg-surface">
              <div
                className="sticky left-0 z-40 flex shrink-0 items-center border-b border-r border-subtle bg-surface px-5 text-[11px] font-semibold uppercase tracking-[0.08em] text-tertiary"
                style={{ width: LABEL }}
              >
                {strings.taskColName}
              </div>
              <div className="flex border-b border-subtle bg-raised/35" style={{ width: total }}>
                {dates.map((d, i) => {
                  const isToday = d.getTime() === today.getTime();
                  const weekend = d.getDay() === 0 || d.getDay() === 6;
                  return (
                    <div
                      key={i}
                      className={`relative flex shrink-0 flex-col items-center justify-center border-l border-subtle text-[11px] tabular-nums ${weekend ? "bg-raised/70" : ""} ${isToday ? "bg-accent-soft text-accent" : "text-tertiary"}`}
                      style={{ width: COL }}
                    >
                      <span className="text-[10px] font-semibold uppercase">{weekdayFmt.format(d)}</span>
                      <span className={`mt-0.5 grid size-6 place-items-center rounded-full font-semibold ${isToday ? "bg-accent text-on-accent" : ""}`}>{dayFmt.format(d)}</span>
                    </div>
                  );
                })}
              </div>
            </div>

      <div className="relative flex flex-col">
        <div className="pointer-events-none absolute bottom-0 z-0 flex" style={{ left: LABEL, top: 0, width: total }} aria-hidden="true">
          {dates.map((d, i) => {
            const weekend = d.getDay() === 0 || d.getDay() === 6;
            const isToday = d.getTime() === today.getTime();
            return <span key={i} className={`h-full shrink-0 border-l border-subtle ${weekend ? "bg-raised/40" : ""} ${isToday ? "bg-accent-soft" : ""}`} style={{ width: COL }} />;
          })}
        </div>
        {arrows.length > 0 && (
          <svg
            className="pointer-events-none absolute left-0 top-0 z-20 overflow-visible"
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
                  className="fill-none stroke-tertiary opacity-55 [stroke-width:1.5]"
                  markerEnd="url(#tl-arrowhead)"
                />
              );
            })}
          </svg>
        )}
        {scheduled.map((t) => {
          const { left, width } = bar(t);
          return (
            <div key={t.id} className="relative isolate flex border-b border-subtle last:border-b-0" style={{ height: ROW }}>
              <button
                type="button"
                className="sticky left-0 z-20 flex shrink-0 overflow-hidden whitespace-nowrap border-r border-subtle bg-surface text-left text-sm font-medium text-primary shadow-[4px_0_8px_-8px_rgba(15,35,55,0.45)] transition-colors hover:bg-raised hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent"
                style={{ width: LABEL }}
                title={t.title}
                onClick={() => onOpen(t.id)}
              >
                <span className="flex min-h-[3.1875rem] w-full items-center gap-3 px-4">
                  {t.assignee !== null && <Avatar email={t.assignee} />}
                  <span className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{t.title}</span>
                  <span className="shrink-0 pl-2 text-[11px] font-normal tabular-nums text-tertiary">{dueLabel(t.dueAt as string)}</span>
                </span>
              </button>
              <div className="relative h-full overflow-hidden" style={{ width: total }}>
                <button
                  type="button"
                  className="absolute top-4 z-0 h-5 overflow-hidden rounded-full border shadow-sm transition-[filter,box-shadow] hover:brightness-95 hover:shadow-md focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2"
                  style={{
                    left,
                    width,
                    minWidth: 18,
                    borderColor: `color-mix(in srgb, ${statusColor(t.status)} 72%, transparent)`,
                    background: statusColor(t.status),
                  }}
                  onClick={() => onOpen(t.id)}
                  title={t.title}
                  aria-label={`${t.title}: ${columnLabel(t.status)}`}
                />
              </div>
            </div>
          );
        })}
      </div>
          </div>
        </div>
      </section>

      {unscheduled.length > 0 && (
        <section className="mt-4 rounded-2xl border border-subtle bg-surface p-4 shadow-sm">
          <div className="mb-3 flex items-center gap-2 text-sm font-semibold text-primary"><Clock3 size={16} className="text-tertiary" />{strings.taskUnscheduled}</div>
          <div className="flex flex-wrap gap-2">
          {unscheduled.map((t) => (
            <button
              key={t.id}
              type="button"
              className="rounded-lg border border-subtle bg-raised px-3 py-2 text-left text-sm font-medium text-primary hover:border-default hover:bg-surface focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
              onClick={() => onOpen(t.id)}
            >
              {t.title}
            </button>
          ))}
          </div>
        </section>
      )}
    </div>
  );
}
