// The Agenda (and single-Day) view: events as a chronological list grouped by
// day — the fast "what's coming" read that a grid can't give. Day tab shows one
// day; Agenda tab shows the loaded range from the anchor forward.
import { getLocale, strings } from "../i18n";
import type { CalendarEvent } from "../jmap";
import { addDays, sameDay, startOfDay } from "./dates";
import styles from "./AgendaModule.module.css";

interface Props {
  from: Date;
  to: Date;
  today: Date;
  events: CalendarEvent[];
  colorOf: (calendarId: string) => string;
  onEventClick: (event: CalendarEvent) => void;
}

function hm(iso: string, locale: string): string {
  return new Date(iso).toLocaleTimeString(locale, {
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function AgendaListView({
  from,
  to,
  today,
  events,
  colorOf,
  onEventClick,
}: Props) {
  const locale = getLocale();
  const inRange = events
    .filter(
      (e) =>
        new Date(e.startsAt) >= startOfDay(from) && new Date(e.startsAt) < to,
    )
    .sort((a, b) => a.startsAt.localeCompare(b.startsAt));

  // Group by calendar day.
  const groups: { day: Date; items: CalendarEvent[] }[] = [];
  for (const e of inRange) {
    const d = startOfDay(new Date(e.startsAt));
    const last = groups[groups.length - 1];
    if (last !== undefined && sameDay(last.day, d)) last.items.push(e);
    else groups.push({ day: d, items: [e] });
  }

  const dayLabel = (d: Date): string => {
    if (sameDay(d, today)) return strings.homeTaskToday;
    if (sameDay(d, addDays(today, 1))) return strings.agendaTomorrow;
    return new Intl.DateTimeFormat(locale, {
      weekday: "long",
      day: "numeric",
      month: "long",
    }).format(d);
  };

  if (groups.length === 0) {
    return (
      <p className={styles.agendaListEmpty}>{strings.agendaNothingUpcoming}</p>
    );
  }

  return (
    <div className={styles.agendaList}>
      {groups.map((g) => (
        <div key={g.day.toISOString()} className={styles.agendaListGroup}>
          <div className={styles.agendaListDay}>
            <span className={styles.agendaListDayNum}>{g.day.getDate()}</span>
            <span className={styles.agendaListDayName}>{dayLabel(g.day)}</span>
          </div>
          <ul className={styles.agendaListItems}>
            {g.items.map((e, i) => (
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
                    <span className={styles.evTitle}>
                      {e.summary.trim().length > 0
                        ? e.summary
                        : strings.agendaUntitledEvent}
                    </span>
                    {e.location !== null && e.location.length > 0 && (
                      <span className={styles.evLoc}>{e.location}</span>
                    )}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      ))}
    </div>
  );
}
