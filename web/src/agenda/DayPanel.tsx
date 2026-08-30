// The right-hand day panel: the selected day's schedule (a timeline of its
// events) and what's coming up after it. Pure presentation over the events the
// module already loaded — clicking an entry opens it in the event editor.
//
// One meeting at a time can also be put in focus here (A8.4), which shows its
// agent under the list: what @agenda can do with that meeting, and a question
// about it answered in place. Opening the entry still opens the editor; the
// focus button beside it is the second, additive way in.
import { useState } from "react";
import { Bot } from "lucide-react";

import { RecordAgentPanel } from "../agents";
import { getLocale, strings } from "../i18n";
import type { CalendarEvent } from "../jmap";
import { awayNames, localDayKey, type AbsentColleague } from "./absences";
import { addDays, sameDay, startOfDay } from "./dates";
import styles from "./AgendaModule.module.css";

/** The media query under which `AgendaModule.module.css` hides `.dayPanel`
 *  outright — written once here so the event editor knows exactly when the
 *  meeting in focus has nowhere else to live and must carry the agent itself.
 *  Change the stylesheet's `@media (max-width: 1100px)` and change this. */
export const DAY_PANEL_HIDDEN = "(max-width: 1100px)";

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
  // The meeting in focus, by the row's own key: one series can appear twice in
  // the same panel (today's sitting and next week's), and they are different
  // records to ask about.
  const [focused, setFocused] = useState<{ key: string; event: CalendarEvent } | null>(null);
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

  /** The focus toggle beside an entry, and — when it is the one in focus —
   *  that meeting's agent under the list it was picked from. */
  function focusButton(key: string, event: CalendarEvent) {
    const on = focused?.key === key;
    return (
      <button
        type="button"
        className={`flex w-9 shrink-0 items-center justify-center rounded-lg border-0 ${
          on ? "bg-accent-soft text-accent" : "bg-transparent text-tertiary hover:bg-raised hover:text-primary"
        }`}
        aria-pressed={on}
        aria-label={strings.recordAgentFocusRecord(titleOf(event))}
        title={strings.recordAgentPanelToggle}
        onClick={() => setFocused(on ? null : { key, event })}
      >
        <Bot size={15} />
      </button>
    );
  }

  /** The agent of the meeting in focus, when it was picked from `keys`. */
  function agentFor(keys: readonly string[]) {
    if (focused === null || !keys.includes(focused.key)) return null;
    return (
      <RecordAgentPanel
        product="agenda"
        recordKind="event"
        recordId={focused.event.id}
        recordLabel={titleOf(focused.event)}
        // A calendar event carries no source of its own: nothing in
        // `/calendar/events` says which mail, room or person it grew out of,
        // so the panel says it does not know rather than inventing one. It
        // gains a sentence the day the read joins `record_origins` (A4.5).
        origin={null}
      />
    );
  }

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
              <li key={`${e.id}-${i}`} className="flex items-stretch gap-1">
                <button
                  type="button"
                  className={`${styles.evItem} min-w-0 flex-1`}
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
                {focusButton(`${e.id}-${i}`, e)}
              </li>
            ))}
          </ul>
        )}
        {agentFor(dayEvents.map((e, i) => `${e.id}-${i}`))}
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
              <li key={`${e.id}-up-${i}`} className="flex items-stretch gap-1">
                <button
                  type="button"
                  className={`${styles.evItem} min-w-0 flex-1`}
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
                {focusButton(`${e.id}-up-${i}`, e)}
              </li>
            ))}
          </ul>
        )}
        {agentFor(upcoming.map((e, i) => `${e.id}-up-${i}`))}
      </section>
    </aside>
  );
}
