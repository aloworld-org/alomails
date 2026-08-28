// The month grid — 6 weeks, Monday-first, à la Google/Outlook. Each day shows
// its events as pills; click a day to add one, click a pill to edit.
import { getLocale, strings } from "../i18n";
import type { CalendarEvent } from "../jmap";
import {
  awayCellText,
  awayNames,
  localDayKey,
  type AbsentColleague,
} from "./absences";
import { eventOnDay, monthGridDays, sameDay, startOfMonth } from "./dates";
import styles from "./AgendaModule.module.css";

interface Props {
  anchor: Date;
  today: Date;
  events: CalendarEvent[];
  absences: ReadonlyMap<string, AbsentColleague[]>;
  selectedDay: Date;
  colorOf: (calendarId: string) => string;
  onDayClick: (day: Date) => void;
  onEventClick: (event: CalendarEvent) => void;
}

const MAX_PER_DAY = 3;

export function MonthView({
  anchor,
  today,
  events,
  absences,
  selectedDay,
  colorOf,
  onDayClick,
  onEventClick,
}: Props) {
  const locale = getLocale();
  const days = monthGridDays(anchor);
  const month = startOfMonth(anchor).getMonth();
  const weekdayFmt = new Intl.DateTimeFormat(locale, { weekday: "short" });
  const timeFmt = new Intl.DateTimeFormat(locale, {
    hour: "numeric",
    minute: "2-digit",
  });
  // A reference Monday (2024-01-01) to label the weekday header row.
  const header = Array.from({ length: 7 }, (_, i) =>
    weekdayFmt.format(new Date(2024, 0, 1 + i)),
  );

  return (
    <div className={styles.month}>
      <div className={styles.monthHead}>
        {header.map((w) => (
          <div key={w} className={styles.weekdayCell}>
            {w}
          </div>
        ))}
      </div>
      <div className={styles.monthGrid}>
        {days.map((day) => {
          const dayEvents = events
            .map((e) => ({
              e,
              s: new Date(e.startsAt),
              en: new Date(e.endsAt),
            }))
            .filter(({ s, en }) => eventOnDay(s, en, day))
            .sort((a, b) =>
              a.e.allDay === b.e.allDay
                ? a.s.getTime() - b.s.getTime()
                : a.e.allDay
                  ? -1
                  : 1,
            );
          const away = absences.get(localDayKey(day)) ?? [];
          const isToday = sameDay(day, today);
          const isSelected = sameDay(day, selectedDay);
          const otherMonth = day.getMonth() !== month;
          return (
            <div
              key={day.toISOString()}
              className={`${styles.dayCell} ${otherMonth ? styles.otherMonth : ""} ${isSelected ? styles.daySelected : ""}`}
              onClick={() => onDayClick(day)}
              onKeyDown={(event) => {
                if (event.key !== "Enter" && event.key !== " ") return;
                event.preventDefault();
                onDayClick(day);
              }}
              role="button"
              tabIndex={0}
              aria-label={`${strings.agendaNewEvent}: ${day.toLocaleDateString(locale)}`}
            >
              <div className={styles.dayNumRow}>
                <span
                  className={`${styles.dayNum} ${isToday ? styles.todayNum : ""}`}
                >
                  {day.getDate()}
                </span>
              </div>
              <div className={styles.dayEvents}>
                {away.length > 0 && (
                  <span
                    className={styles.awayPill}
                    title={strings.agendaAwayTitle(awayNames(away))}
                    aria-label={strings.agendaAwayTitle(awayNames(away))}
                  >
                    {awayCellText(away)}
                  </span>
                )}
                {dayEvents.slice(0, MAX_PER_DAY).map(({ e, s }) => (
                  <button
                    key={`${e.id}-${e.startsAt}`}
                    className={`${styles.eventPill} ${e.allDay ? styles.eventAllDay : styles.eventTimed}`}
                    style={
                      {
                        ["--cal"]: colorOf(e.calendarId),
                      } as React.CSSProperties
                    }
                    onClick={(ev) => {
                      ev.stopPropagation();
                      onEventClick(e);
                    }}
                    title={e.summary}
                  >
                    {e.allDay ? (
                      <span className={styles.eventTitle}>
                        {e.summary || strings.agendaUntitledEvent}
                      </span>
                    ) : (
                      <>
                        <span className={styles.eventDot} aria-hidden />
                        <span className={styles.eventTime}>
                          {timeFmt.format(s)}
                        </span>
                        <span className={styles.eventTitle}>
                          {e.summary || strings.agendaUntitledEvent}
                        </span>
                      </>
                    )}
                  </button>
                ))}
                {dayEvents.length > MAX_PER_DAY && (
                  <span className={styles.moreEvents}>
                    +{dayEvents.length - MAX_PER_DAY}
                  </span>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
