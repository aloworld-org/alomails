// The week time-grid — Monday-first, an hour gutter and 7 day columns, à la
// Google/Outlook. Timed events are positioned blocks; all-day events sit in a
// strip above the grid. Click empty time to add an event at that hour.
import { getLocale } from "../i18n";
import type { CalendarEvent } from "../jmap";
import {
  HOURS_IN_DAY,
  eventOnDay,
  hourFraction,
  sameDay,
  startOfDay,
  weekDays,
} from "./dates";
import styles from "./AgendaModule.module.css";

interface Props {
  anchor: Date;
  today: Date;
  events: CalendarEvent[];
  onSlotClick: (at: Date) => void;
  onEventClick: (event: CalendarEvent) => void;
}

const HOUR_HEIGHT = 46; // px per hour

export function WeekView({
  anchor,
  today,
  events,
  onSlotClick,
  onEventClick,
}: Props) {
  const locale = getLocale();
  const days = weekDays(anchor);
  const dayFmt = new Intl.DateTimeFormat(locale, { weekday: "short" });
  const timeFmt = new Intl.DateTimeFormat(locale, {
    hour: "numeric",
    minute: "2-digit",
  });
  const hourFmt = new Intl.DateTimeFormat(locale, { hour: "numeric" });

  const parsed = events.map((e) => ({
    e,
    s: new Date(e.startsAt),
    en: new Date(e.endsAt),
  }));
  const allDay = parsed.filter((p) => p.e.allDay);
  const timed = parsed.filter((p) => !p.e.allDay);

  function slotAt(day: Date, offsetY: number): Date {
    const hour = Math.max(0, Math.min(23, Math.floor(offsetY / HOUR_HEIGHT)));
    const at = startOfDay(day);
    at.setHours(hour, 0, 0, 0);
    return at;
  }

  return (
    <div className={styles.week}>
      <div className={styles.weekHead}>
        <div className={styles.gutterCorner} />
        {days.map((d) => {
          const isToday = sameDay(d, today);
          return (
            <div key={d.toISOString()} className={styles.weekDayHead}>
              <span className={styles.weekDayName}>{dayFmt.format(d)}</span>
              <span
                className={`${styles.weekDayNum} ${isToday ? styles.todayNum : ""}`}
              >
                {d.getDate()}
              </span>
            </div>
          );
        })}
      </div>

      {allDay.length > 0 && (
        <div className={styles.allDayStrip}>
          <div className={styles.gutterCorner}> </div>
          {days.map((day) => (
            <div key={day.toISOString()} className={styles.allDayCol}>
              {allDay
                .filter(({ s, en }) => eventOnDay(s, en, day))
                .map(({ e }) => (
                  <button
                    key={`${e.id}-${e.startsAt}`}
                    className={`${styles.chip} ${styles.chipAllDay}`}
                    onClick={() => onEventClick(e)}
                    title={e.summary}
                  >
                    <span className={styles.chipTitle}>{e.summary || "—"}</span>
                  </button>
                ))}
            </div>
          ))}
        </div>
      )}

      <div className={styles.weekBody}>
        <div className={styles.hourGutter}>
          {Array.from({ length: HOURS_IN_DAY }, (_, h) => (
            <div
              key={h}
              className={styles.hourLabel}
              style={{ height: HOUR_HEIGHT }}
            >
              {h > 0 && hourFmt.format(new Date(2024, 0, 1, h))}
            </div>
          ))}
        </div>
        {days.map((day) => {
          const dayStart = startOfDay(day).getTime();
          const dayEnd = dayStart + 86400000;
          return (
            <div
              key={day.toISOString()}
              className={styles.dayColumn}
              style={{ height: HOUR_HEIGHT * HOURS_IN_DAY }}
              onClick={(ev) => {
                const rect = ev.currentTarget.getBoundingClientRect();
                onSlotClick(slotAt(day, ev.clientY - rect.top));
              }}
            >
              {Array.from({ length: HOURS_IN_DAY }, (_, h) => (
                <div
                  key={h}
                  className={styles.hourLine}
                  style={{ top: h * HOUR_HEIGHT }}
                />
              ))}
              {timed
                .filter(
                  ({ s, en }) =>
                    s.getTime() < dayEnd && en.getTime() > dayStart,
                )
                .map(({ e, s, en }) => {
                  const top = Math.max(0, hourFraction(s)) * HOUR_HEIGHT;
                  const endFrac =
                    en.getTime() >= dayEnd ? HOURS_IN_DAY : hourFraction(en);
                  const height = Math.max(
                    18,
                    (endFrac - Math.max(0, hourFraction(s))) * HOUR_HEIGHT - 2,
                  );
                  return (
                    <button
                      key={`${e.id}-${e.startsAt}`}
                      className={styles.eventBlock}
                      style={{ top, height }}
                      onClick={(clickEv) => {
                        clickEv.stopPropagation();
                        onEventClick(e);
                      }}
                      title={e.summary}
                    >
                      <span className={styles.blockTitle}>
                        {e.summary || "—"}
                      </span>
                      <span className={styles.blockTime}>
                        {timeFmt.format(s)}
                      </span>
                    </button>
                  );
                })}
            </div>
          );
        })}
      </div>
    </div>
  );
}
