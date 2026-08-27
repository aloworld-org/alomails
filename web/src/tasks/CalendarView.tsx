// Calendar: tasks placed on a month grid by their real due date. Nav by month;
// click a task to open it. Tasks without a due date aren't placed (a count is
// shown). Reuses the agenda's month-grid date helpers.
import { useMemo, useState } from "react";
import { CalendarDays, ChevronLeft, ChevronRight, Plus } from "lucide-react";

import { getLocale, strings } from "../i18n";
import type { Task } from "../jmap";
import { addMonths, monthGridDays, sameDay, startOfDay, startOfMonth } from "../agenda/dates";
import { statusColor } from "./parts";

const MAX = 3;

interface Props {
  tasks: Task[];
  onOpen: (id: string) => void;
  onAdd: (day: Date) => void;
}

export function CalendarView({ tasks, onOpen, onAdd }: Props) {
  const locale = getLocale();
  const today = useMemo(() => startOfDay(new Date()), []);
  const [anchor, setAnchor] = useState<Date>(today);

  const days = monthGridDays(anchor);
  const month = startOfMonth(anchor).getMonth();
  const weekdayFmt = new Intl.DateTimeFormat(locale, { weekday: "short" });
  const header = Array.from({ length: 7 }, (_, i) => weekdayFmt.format(new Date(2024, 0, 1 + i)));
  const label = new Intl.DateTimeFormat(locale, { month: "long", year: "numeric" }).format(anchor);
  const undated = tasks.filter((t) => t.dueAt === null).length;
  const monthTasks = tasks.filter((task) => {
    if (task.dueAt === null) return false;
    const due = new Date(task.dueAt);
    return due.getMonth() === month && due.getFullYear() === anchor.getFullYear();
  }).length;

  return (
    <div className="mx-auto w-full max-w-[100rem] px-6 pb-8 pt-6 max-sm:px-4">
      <section className="overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm">
        <header className="flex flex-wrap items-center justify-between gap-4 border-b border-subtle px-5 py-4">
          <div className="flex min-w-0 items-center gap-3">
            <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-[var(--accent-soft)] text-accent" aria-hidden="true">
              <CalendarDays size={19} />
            </span>
            <div className="min-w-0">
              <h2 className="m-0 truncate text-base font-semibold capitalize text-primary">{label}</h2>
              <p className="m-0 mt-0.5 text-xs text-tertiary">{strings.taskSummaryTotal(monthTasks)}</p>
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            {undated > 0 && <span className="mr-2 rounded-full bg-raised px-2.5 py-1 text-xs font-medium text-secondary">{strings.taskUnscheduled}: {undated}</span>}
            <button type="button" className="rounded-lg border border-default bg-surface text-sm font-semibold text-primary shadow-sm transition-colors hover:bg-raised focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent" onClick={() => setAnchor(today)}>
              <span className="flex min-h-9 items-center px-3">{strings.agendaToday}</span>
            </button>
            <div className="flex items-center overflow-hidden rounded-lg border border-default bg-surface shadow-sm">
              <button type="button" className="text-secondary transition-colors hover:bg-raised hover:text-primary focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent" onClick={() => setAnchor((a) => addMonths(a, -1))} aria-label={strings.agendaPrev}>
                <span className="grid size-9 place-items-center"><ChevronLeft size={17} /></span>
              </button>
              <span className="h-5 w-px bg-[var(--border-subtle)]" aria-hidden="true" />
              <button type="button" className="text-secondary transition-colors hover:bg-raised hover:text-primary focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent" onClick={() => setAnchor((a) => addMonths(a, 1))} aria-label={strings.agendaNext}>
                <span className="grid size-9 place-items-center"><ChevronRight size={17} /></span>
              </button>
            </div>
          </div>
        </header>

        <div className="grid grid-cols-7 border-b border-subtle bg-raised/45">
          {header.map((w, index) => (
            <div key={w} className={`px-3 py-2.5 text-right text-[11px] font-semibold uppercase tracking-[0.08em] text-tertiary ${index > 4 ? "bg-raised/55" : ""}`}>
              {w}
            </div>
          ))}
        </div>

      <div className="grid min-h-[42rem] grid-cols-7 grid-rows-6">
        {days.map((day) => {
          const dayTasks = tasks.filter((t) => t.dueAt !== null && sameDay(new Date(t.dueAt as string), day));
          const isToday = sameDay(day, today);
          const otherMonth = day.getMonth() !== month;
          const weekend = day.getDay() === 0 || day.getDay() === 6;
          return (
            <div
              key={day.toISOString()}
              className={`group/day relative flex cursor-pointer flex-col gap-1 overflow-hidden border-b border-r border-subtle p-2 transition-colors hover:bg-raised focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent ${otherMonth ? "bg-app text-tertiary" : weekend ? "bg-raised/25" : "bg-surface"} ${isToday ? "bg-[var(--accent-soft)]" : ""}`}
              role="button"
              tabIndex={0}
              aria-label={strings.taskCreateOnDate(day.toLocaleDateString(locale))}
              onClick={() => onAdd(day)}
              onKeyDown={(event) => {
                if (event.target !== event.currentTarget) return;
                if (event.key !== "Enter" && event.key !== " ") return;
                event.preventDefault();
                onAdd(day);
              }}
            >
              <div className="flex items-center justify-between">
                <span className={`flex size-7 items-center justify-center rounded-full text-xs font-semibold tabular-nums ${isToday ? "bg-accent text-on-accent shadow-sm" : otherMonth ? "text-tertiary" : "text-secondary"}`}>{day.getDate()}</span>
                <span className="grid size-6 place-items-center rounded-md text-tertiary opacity-0 transition-opacity group-hover/day:opacity-100" aria-hidden="true"><Plus size={14} /></span>
              </div>
              <div className="mt-0.5 flex min-w-0 flex-col gap-1">
                {dayTasks.slice(0, MAX).map((t) => (
                  <button
                    key={t.id}
                    type="button"
                    className="w-full overflow-hidden rounded-lg border text-left text-xs font-medium text-primary shadow-sm transition-[filter,box-shadow] hover:brightness-95 hover:shadow-md focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent"
                    style={{
                      borderColor: `color-mix(in srgb, ${statusColor(t.status)} 28%, transparent)`,
                      background: `color-mix(in srgb, ${statusColor(t.status)} 10%, var(--surface))`,
                    }}
                    onClick={(event) => {
                      event.stopPropagation();
                      onOpen(t.id);
                    }}
                    title={t.title}
                  >
                    <span className="flex min-h-7 items-center gap-2 px-2">
                      <span className="h-3.5 w-[3px] shrink-0 rounded-full" style={{ background: statusColor(t.status) }} aria-hidden />
                      <span className={`truncate ${t.status === "done" ? "text-tertiary line-through" : ""}`}>{t.title}</span>
                    </span>
                  </button>
                ))}
                {dayTasks.length > MAX && <span className="pl-2 text-[11px] font-medium text-tertiary">+{dayTasks.length - MAX}</span>}
              </div>
            </div>
          );
        })}
      </div>
      </section>
    </div>
  );
}
