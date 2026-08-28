// The right-hand day panel: the selected day's schedule (a timeline of its
// events) and what's coming up after it. Pure presentation over the events the
// module already loaded — clicking an entry opens it in the event editor.
import { getLocale, strings } from "../i18n";
import type { CalendarEvent } from "../jmap";
import { awayNames, localDayKey, type AbsentColleague } from "./absences";
import { addDays, sameDay, startOfDay } from "./dates";
import styles from "./AgendaModule.module.css";

interface Props {
  day: Date;
  today: Date;
  events: CalendarEvent[];
  absences: ReadonlyMap<string, AbsentColleague[]>;
  colorOf: (calendarId: string) => string;
  onEventClick: (event: CalendarEvent) => void;
}

function hm(iso: string, locale: string): string {
  return new Date(iso).toLocaleTimeString(locale, {
    hour: "2-digit",
    minute: "2-digit",
  });
}

function titleOf(e: CalendarEvent): string {
  return e.summary.trim().length > 0 ? e.summary : strings.agendaUntitledEvent;
}

export function DayPanel({
  day,
  today,
  events,
  absences,
  colorOf,
  onEventClick,
}: Props) {
  const locale = getLocale();
  const start = startOfDay(day);
  const end = addDays(start, 1);
  const away = absences.get(localDayKey(day)) ?? [];

  const dayEvents = events
    .filter((e) => new Date(e.startsAt) < end && new Date(e.endsAt) > start)
    .sort((a, b) => a.startsAt.localeCompare(b.startsAt));

  // "Upcoming": the next events strictly after the selected day, chronological.
  const upcoming = events
    .filter((e) => new Date(e.startsAt) >= end)
    .sort((a, b) => a.startsAt.localeCompare(b.startsAt))
    .slice(0, 4);

  const dateLabel = new Intl.DateTimeFormat(locale, {
    weekday: "long",
    month: "long",
    day: "numeric",
  }).format(day);

  const relDay = (iso: string): string => {
    const d = new Date(iso);
    if (sameDay(d, today)) return strings.homeTaskToday;
    if (sameDay(d, addDays(today, 1))) return strings.agendaTomorrow;
    return new Intl.DateTimeFormat(locale, {
      weekday: "long",
      month: "short",
      day: "numeric",
    }).format(d);
  };

  return (
    <aside className={styles.dayPanel}>
      <div className={styles.dayPanelHead}>
        <h2 className={styles.dayPanelDate}>{dateLabel}</h2>
      </div>

      <section className={styles.panelSection}>
        <div className={styles.panelSectionHead}>
          <span>{sameDay(day, today) ? strings.homeTaskToday : dateLabel}</span>
          <span className={styles.panelCount}>
            {strings.agendaEventCount(dayEvents.length)}
          </span>
        </div>
        {away.length > 0 && (
          <p className={styles.awayLine} title={awayNames(away)}>
            <span className={styles.awayLabel}>{strings.agendaAway}</span>
            <span className={styles.awayNames}>{awayNames(away)}</span>
          </p>
        )}
        {dayEvents.length === 0 ? (
          <p className={styles.panelEmpty}>{strings.homeNoEventsToday}</p>
        ) : (
          <ul className={styles.panelList}>
            {dayEvents.map((e, i) => (
              <li key={`${e.id}-${i}`}>
                <button
                  type="button"
                  className={styles.evItem}
                  onClick={() => onEventClick(e)}
                >
                  <span
                    className={styles.evBar}
                    style={{ background: colorOf(e.calendarId) }}
                    aria-hidden
                  />
                  <span className={styles.evBody}>
                    <span className={styles.evTime}>
                      {e.allDay
                        ? strings.agendaAllDay
                        : `${hm(e.startsAt, locale)} – ${hm(e.endsAt, locale)}`}
                    </span>
                    <span className={styles.evTitle}>{titleOf(e)}</span>
                    {e.location !== null && e.location.length > 0 && (
                      <span className={styles.evLoc}>{e.location}</span>
                    )}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className={styles.panelSection}>
        <div className={styles.panelSectionHead}>
          <span>{strings.agendaUpcoming}</span>
          <span className={styles.panelCount}>
            {strings.agendaEventCount(upcoming.length)}
          </span>
        </div>
        {upcoming.length === 0 ? (
          <p className={styles.panelEmpty}>{strings.agendaNothingUpcoming}</p>
        ) : (
          <ul className={styles.panelList}>
            {upcoming.map((e, i) => (
              <li key={`${e.id}-up-${i}`}>
                <button
                  type="button"
                  className={styles.evItem}
                  onClick={() => onEventClick(e)}
                >
                  <span
                    className={styles.evBar}
                    style={{ background: colorOf(e.calendarId) }}
                    aria-hidden
                  />
                  <span className={styles.evBody}>
                    <span className={styles.evDay}>{relDay(e.startsAt)}</span>
                    <span className={styles.evTime}>
                      {e.allDay
                        ? strings.agendaAllDay
                        : `${hm(e.startsAt, locale)} – ${hm(e.endsAt, locale)}`}
                    </span>
                    <span className={styles.evTitle}>{titleOf(e)}</span>
                    {e.location !== null && e.location.length > 0 && (
                      <span className={styles.evLoc}>{e.location}</span>
                    )}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </aside>
  );
}
