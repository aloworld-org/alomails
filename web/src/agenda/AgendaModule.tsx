// The Calendar (Agenda) module — the Outlook/Google-style shell: a left sidebar
// with a "New event" button and a mini-month navigator, and a main area with a
// toolbar (Today, prev/next, period label, Month/Week switch) over the active
// view. All data goes through the authenticated /calendar API on the store; the
// UI works in local time and converts at the edges.
import { useCallback, useEffect, useMemo, useState } from "react";
import { CalendarPlus, ChevronLeft, ChevronRight } from "lucide-react";

import { getLocale, strings } from "../i18n";
import { useJmapClient, type CalendarEvent, type EventInput } from "../jmap";
import { Spinner } from "../ds";
import { EventModal } from "./EventModal";
import { MiniMonth } from "./MiniMonth";
import { MonthView } from "./MonthView";
import { WeekView } from "./WeekView";
import { addDays, addMonths, startOfDay, startOfMonth, startOfWeek, weekDays } from "./dates";
import styles from "./AgendaModule.module.css";

type View = "month" | "week";
type Editing = { event: CalendarEvent | null; initialStart: Date } | null;

export function AgendaModule() {
  const client = useJmapClient();
  const locale = getLocale();
  const today = useMemo(() => startOfDay(new Date()), []);
  const [view, setView] = useState<View>("month");
  const [anchor, setAnchor] = useState<Date>(today);
  const [events, setEvents] = useState<CalendarEvent[]>([]);
  const [loading, setLoading] = useState(true);
  const [editing, setEditing] = useState<Editing>(null);

  // The visible [from, to) window for the current view.
  const [from, to] = useMemo<[Date, Date]>(() => {
    if (view === "month") {
      const first = startOfWeek(startOfMonth(anchor)); // 6-week grid start
      return [first, addDays(first, 42)];
    }
    const first = startOfWeek(anchor);
    return [first, addDays(first, 7)];
  }, [view, anchor]);

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      const list = await client.calendarEvents(from.toISOString(), to.toISOString());
      setEvents(list);
    } catch {
      setEvents([]);
    } finally {
      setLoading(false);
    }
  }, [client, from, to]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const label = useMemo(() => {
    if (view === "month") {
      return new Intl.DateTimeFormat(locale, { month: "long", year: "numeric" }).format(anchor);
    }
    const week = weekDays(anchor);
    const fmt = new Intl.DateTimeFormat(locale, { day: "numeric", month: "short" });
    const yr = new Intl.DateTimeFormat(locale, { year: "numeric" }).format(week[6]);
    return `${fmt.format(week[0])} – ${fmt.format(week[6])}, ${yr}`;
  }, [view, anchor, locale]);

  function step(dir: -1 | 1) {
    setAnchor((a) => (view === "month" ? addMonths(a, dir) : addDays(startOfWeek(a), dir * 7)));
  }

  function openNew(at: Date) {
    setEditing({ event: null, initialStart: at });
  }

  // Editing a recurring occurrence edits the whole series — load the stored
  // master (unexpanded) so its base time + rule, not the clicked occurrence's
  // shifted time, drive the form. A one-off opens directly.
  async function openEvent(e: CalendarEvent) {
    if (e.recurrence !== null) {
      try {
        const master = await client.getEvent(e.id);
        setEditing({ event: master, initialStart: new Date(master.startsAt) });
        return;
      } catch {
        /* fall back to the occurrence */
      }
    }
    setEditing({ event: e, initialStart: new Date(e.startsAt) });
  }

  async function save(id: string | null, input: EventInput) {
    if (id === null) await client.createEvent(input);
    else await client.updateEvent(id, input);
    setEditing(null);
    await reload();
  }

  async function remove(id: string) {
    await client.deleteEvent(id);
    setEditing(null);
    await reload();
  }

  // A sensible default start when the user clicks a bare day (9:00 that day).
  function dayAtNine(day: Date): Date {
    const d = startOfDay(day);
    d.setHours(9, 0, 0, 0);
    return d;
  }

  return (
    <div className={styles.agenda}>
      <aside className={styles.sidebar}>
        <button className={styles.newBtn} onClick={() => openNew(dayAtNine(today))}>
          <CalendarPlus size={18} />
          {strings.agendaNewEvent}
        </button>
        <MiniMonth anchor={anchor} today={today} onPick={(d) => setAnchor(d)} />
      </aside>

      <section className={styles.main}>
        <header className={styles.toolbar}>
          <button className={styles.todayBtn} onClick={() => setAnchor(today)}>
            {strings.agendaToday}
          </button>
          <button className={styles.navBtn} onClick={() => step(-1)} aria-label={strings.agendaPrev}>
            <ChevronLeft size={18} />
          </button>
          <button className={styles.navBtn} onClick={() => step(1)} aria-label={strings.agendaNext}>
            <ChevronRight size={18} />
          </button>
          <h1 className={styles.periodLabel}>{label}</h1>
          {loading && <Spinner size={16} />}
          <div className={styles.viewSwitch}>
            <button
              className={view === "month" ? styles.viewActive : ""}
              onClick={() => setView("month")}
            >
              {strings.agendaMonth}
            </button>
            <button
              className={view === "week" ? styles.viewActive : ""}
              onClick={() => setView("week")}
            >
              {strings.agendaWeek}
            </button>
          </div>
        </header>

        <div className={styles.viewport}>
          {view === "month" ? (
            <MonthView
              anchor={anchor}
              today={today}
              events={events}
              onDayClick={(day) => openNew(dayAtNine(day))}
              onEventClick={(e) => void openEvent(e)}
            />
          ) : (
            <WeekView
              anchor={anchor}
              today={today}
              events={events}
              onSlotClick={(at) => openNew(at)}
              onEventClick={(e) => void openEvent(e)}
            />
          )}
        </div>
      </section>

      {editing !== null && (
        <EventModal
          event={editing.event}
          initialStart={editing.initialStart}
          onSave={save}
          onDelete={remove}
          onClose={() => setEditing(null)}
        />
      )}
    </div>
  );
}
