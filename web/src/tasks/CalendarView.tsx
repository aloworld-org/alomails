// Calendar: tasks placed on a month grid by their real due date. Nav by month;
// click a task to open it. Tasks without a due date aren't placed (a count is
// shown). Reuses the agenda's month-grid date helpers.
import { useMemo, useState } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";

import { getLocale, strings } from "../i18n";
import type { Task } from "../jmap";
import { addMonths, monthGridDays, sameDay, startOfDay, startOfMonth } from "../agenda/dates";
import styles from "./TasksModule.module.css";

const MAX = 3;

interface Props {
  tasks: Task[];
  onOpen: (id: string) => void;
}

function prioClass(t: Task): string {
  if (t.status === "done") return styles.tcDotDone ?? "";
  if (t.priority === "high") return styles.tcDotHigh ?? "";
  if (t.priority === "medium") return styles.tcDotMedium ?? "";
  if (t.priority === "low") return styles.tcDotLow ?? "";
  return "";
}

export function CalendarView({ tasks, onOpen }: Props) {
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
    <div className={styles.tcal}>
      <div className={styles.tcalBar}>
        <button type="button" className={styles.tcalToday} onClick={() => setAnchor(today)}>
          {strings.agendaToday}
        </button>
        <button type="button" className={styles.tcalNav} onClick={() => setAnchor((a) => addMonths(a, -1))} aria-label={strings.agendaPrev}>
          <ChevronLeft size={18} />
        </button>
        <button type="button" className={styles.tcalNav} onClick={() => setAnchor((a) => addMonths(a, 1))} aria-label={strings.agendaNext}>
          <ChevronRight size={18} />
        </button>
        <h2 className={styles.tcalLabel}>{label}</h2>
        {undated > 0 && <span className={styles.tcalUndated}>{strings.taskUnscheduled}: {undated}</span>}
      </div>

      <div className={styles.tcalHead}>
        {header.map((w) => (
          <div key={w} className={styles.tcalWeekday}>
            {w}
          </div>
        ))}
      </div>
      <div className={styles.tcalGrid}>
        {days.map((day) => {
          const dayTasks = tasks.filter((t) => t.dueAt !== null && sameDay(new Date(t.dueAt as string), day));
          const isToday = sameDay(day, today);
          const otherMonth = day.getMonth() !== month;
          return (
            <div key={day.toISOString()} className={`${styles.tcalCell} ${otherMonth ? styles.tcalOther : ""}`}>
              <div className={styles.tcalNumRow}>
                <span className={`${styles.tcalNum} ${isToday ? styles.tcalTodayNum : ""}`}>{day.getDate()}</span>
              </div>
              <div className={styles.tcalTasks}>
                {dayTasks.slice(0, MAX).map((t) => (
                  <button
                    key={t.id}
                    type="button"
                    className={`${styles.tcalTask} ${t.status === "done" ? styles.tcalTaskDone : ""}`}
                    onClick={() => onOpen(t.id)}
                    title={t.title}
                  >
                    <span className={`${styles.tcalDot} ${prioClass(t)}`} aria-hidden />
                    <span className={styles.tcalTaskTitle}>{t.title}</span>
                  </button>
                ))}
                {dayTasks.length > MAX && <span className={styles.tcalMore}>+{dayTasks.length - MAX}</span>}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
