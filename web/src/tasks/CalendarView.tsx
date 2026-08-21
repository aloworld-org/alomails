// Calendar: tasks placed on a month grid by their real due date. Nav by month;
// click a task to open it. Tasks without a due date aren't placed (a count is
// shown). Reuses the agenda's month-grid date helpers.
import { useMemo, useState } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";

import { getLocale, strings } from "../i18n";
import type { Task } from "../jmap";
import { addMonths, monthGridDays, sameDay, startOfDay, startOfMonth } from "../agenda/dates";

const MAX = 3;

interface Props {
  tasks: Task[];
  onOpen: (id: string) => void;
  onAdd: (day: Date) => void;
}

function prioClass(t: Task): string {
  if (t.status === "done") return "bg-tertiary";
  if (t.priority === "high") return "bg-danger";
  if (t.priority === "medium") return "bg-warning";
  if (t.priority === "low") return "bg-success";
  return "bg-accent";
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

  return (
    <div className="flex min-h-full flex-col">
      <div className="flex items-center gap-2 px-6 py-3">
        <button type="button" className="rounded-lg border border-default px-3.5 py-1.5 text-sm font-medium text-primary transition-colors hover:bg-raised focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent" onClick={() => setAnchor(today)}>
          {strings.agendaToday}
        </button>
        <button type="button" className="rounded-lg p-1.5 text-secondary transition-colors hover:bg-raised hover:text-primary focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent" onClick={() => setAnchor((a) => addMonths(a, -1))} aria-label={strings.agendaPrev}>
          <ChevronLeft size={18} />
        </button>
        <button type="button" className="rounded-lg p-1.5 text-secondary transition-colors hover:bg-raised hover:text-primary focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent" onClick={() => setAnchor((a) => addMonths(a, 1))} aria-label={strings.agendaNext}>
          <ChevronRight size={18} />
        </button>
        <h2 className="mx-2 text-lg font-semibold capitalize text-primary">{label}</h2>
        {undated > 0 && <span className="ml-auto rounded-full bg-muted px-2.5 py-1 text-sm text-tertiary">{strings.taskUnscheduled}: {undated}</span>}
      </div>

      <div className="grid grid-cols-7 border-b border-subtle px-6">
        {header.map((w) => (
          <div key={w} className="px-2.5 py-1.5 text-right text-[0.72rem] font-semibold uppercase tracking-[0.03em] text-tertiary">
            {w}
          </div>
        ))}
      </div>
      <div className="mx-6 mb-6 grid min-h-[720px] flex-1 grid-cols-7 grid-rows-6 overflow-hidden rounded-xl border-l border-t border-subtle">
        {days.map((day) => {
          const dayTasks = tasks.filter((t) => t.dueAt !== null && sameDay(new Date(t.dueAt as string), day));
          const isToday = sameDay(day, today);
          const otherMonth = day.getMonth() !== month;
          return (
            <div
              key={day.toISOString()}
              className={`flex cursor-pointer flex-col gap-1 overflow-hidden border-b border-r border-subtle p-1.5 transition-colors hover:bg-raised focus-visible:outline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent ${otherMonth ? "bg-app" : "bg-surface"}`}
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
              <div className="flex justify-end">
                <span className={`flex size-6 items-center justify-center rounded-full text-sm tabular-nums ${isToday ? "bg-accent font-semibold text-on-accent" : "text-secondary"}`}>{day.getDate()}</span>
              </div>
              <div className="flex min-w-0 flex-col gap-0.5">
                {dayTasks.slice(0, MAX).map((t) => (
                  <button
                    key={t.id}
                    type="button"
                    className="flex w-full items-center gap-1.5 rounded-md px-1.5 py-1 text-left text-[0.72rem] text-primary transition-colors hover:bg-muted focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent"
                    onClick={(event) => {
                      event.stopPropagation();
                      onOpen(t.id);
                    }}
                    title={t.title}
                  >
                    <span className={`size-[7px] shrink-0 rounded-full ${prioClass(t)}`} aria-hidden />
                    <span className={`truncate ${t.status === "done" ? "text-tertiary line-through" : ""}`}>{t.title}</span>
                  </button>
                ))}
                {dayTasks.length > MAX && <span className="pl-1 text-[0.68rem] text-tertiary">+{dayTasks.length - MAX}</span>}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
