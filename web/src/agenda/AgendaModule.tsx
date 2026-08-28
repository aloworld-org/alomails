// The Calendar (Agenda) module — an Outlook/Google-style three-pane shell: a
// left sidebar (New event, mini-month, and the calendar list with colour toggles
// that filter the grid), the main view (Day / Week / Month / Agenda over a
// toolbar), and a right day-panel with the selected day's schedule and what's
// upcoming. All data goes through the authenticated /calendar API; the UI works
// in local time and converts at the edges.
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  CalendarPlus,
  Check,
  ChevronLeft,
  ChevronRight,
  Plus,
  Share2,
  Trash2,
} from "lucide-react";

import { getLocale, strings } from "../i18n";
import {
  useJmapClient,
  type Calendar,
  type CalendarEvent,
  type EventInput,
} from "../jmap";
import {
  Button,
  IconButton,
  Spinner,
  Toolbar,
  ToolbarGroup,
  useDialogs,
} from "../ds";
import { useAbsenceLayer } from "./absences";
import { EventModal } from "./EventModal";
import { ShareDialog } from "./ShareDialog";
import { MiniMonth } from "./MiniMonth";
import { MonthView } from "./MonthView";
import { WeekView } from "./WeekView";
import { AgendaListView } from "./AgendaListView";
import { DayPanel } from "./DayPanel";
import { calendarColorMap } from "./colors";
import {
  addDays,
  addMonths,
  startOfDay,
  startOfMonth,
  startOfWeek,
  weekDays,
} from "./dates";
import styles from "./AgendaModule.module.css";

type View = "day" | "week" | "month" | "agenda";
type Editing = {
  event: CalendarEvent | null;
  initialStart: Date;
  occurrenceStart?: string;
  master?: CalendarEvent;
} | null;

const VIEWS: { id: View; label: () => string }[] = [
  { id: "day", label: () => strings.agendaDay },
  { id: "week", label: () => strings.agendaWeek },
  { id: "month", label: () => strings.agendaMonth },
  { id: "agenda", label: () => strings.agendaAgenda },
];

export function AgendaModule() {
  const client = useJmapClient();
  const { prompt } = useDialogs();
  const locale = getLocale();
  const today = useMemo(() => startOfDay(new Date()), []);
  const [view, setView] = useState<View>("month");
  const [anchor, setAnchor] = useState<Date>(today);
  const [selectedDay, setSelectedDay] = useState<Date>(today);
  const [events, setEvents] = useState<CalendarEvent[]>([]);
  const [calendars, setCalendars] = useState<Calendar[]>([]);
  const [hidden, setHidden] = useState<ReadonlySet<string>>(new Set());
  const [loading, setLoading] = useState(true);
  const [editing, setEditing] = useState<Editing>(null);
  const [sharing, setSharing] = useState<Calendar | null>(null);

  const colorMap = useMemo(() => calendarColorMap(calendars), [calendars]);
  const colorOf = useCallback(
    (id: string) => colorMap.get(id) ?? "#e76f51",
    [colorMap],
  );
  const visibleEvents = useMemo(
    () => events.filter((e) => !hidden.has(e.calendarId)),
    [events, hidden],
  );

  const loadCalendars = useCallback(async () => {
    try {
      setCalendars(await client.calendars());
    } catch {
      /* keep whatever we have */
    }
  }, [client]);

  useEffect(() => {
    void loadCalendars();
  }, [loadCalendars]);

  function toggleCalendar(id: string) {
    setHidden((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  async function newCalendar() {
    const name = (
      await prompt({ message: strings.agendaNewCalendarPrompt })
    )?.trim();
    if (name === undefined || name === "") return;
    try {
      await client.createCalendar(name);
      await loadCalendars();
    } catch {
      /* ignore */
    }
  }

  async function removeCalendar(id: string) {
    try {
      await client.deleteCalendar(id);
      await Promise.all([loadCalendars(), reload()]);
    } catch {
      /* the personal calendar is protected (409) */
    }
  }

  // The window to load: the visible view, widened to at least three weeks so the
  // day-panel's "upcoming" always has something ahead of the selected day.
  const [from, to] = useMemo<[Date, Date]>(() => {
    const base =
      view === "month"
        ? startOfWeek(startOfMonth(anchor))
        : view === "week"
          ? startOfWeek(anchor)
          : startOfDay(anchor);
    const span =
      view === "month" ? 42 : view === "week" ? 7 : view === "agenda" ? 30 : 1;
    return [base, addDays(base, Math.max(span, 21))];
  }, [view, anchor]);

  // Who is away on each visible day — the workspace provides the feed, the
  // standalone mail product has none and the map stays empty (see absences.ts).
  const absences = useAbsenceLayer(from, to);

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      const list = await client.calendarEvents(
        from.toISOString(),
        to.toISOString(),
      );
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
      return new Intl.DateTimeFormat(locale, {
        month: "long",
        year: "numeric",
      }).format(anchor);
    }
    if (view === "day") {
      return new Intl.DateTimeFormat(locale, {
        weekday: "long",
        day: "numeric",
        month: "long",
      }).format(anchor);
    }
    if (view === "agenda") {
      return new Intl.DateTimeFormat(locale, {
        month: "long",
        year: "numeric",
      }).format(anchor);
    }
    const week = weekDays(anchor);
    const fmt = new Intl.DateTimeFormat(locale, {
      day: "numeric",
      month: "short",
    });
    const yr = new Intl.DateTimeFormat(locale, { year: "numeric" }).format(
      week[6],
    );
    return `${fmt.format(week[0])} – ${fmt.format(week[6])}, ${yr}`;
  }, [view, anchor, locale]);

  function step(dir: -1 | 1) {
    setAnchor((a) => {
      if (view === "month" || view === "agenda") return addMonths(a, dir);
      if (view === "day") return addDays(startOfDay(a), dir);
      return addDays(startOfWeek(a), dir * 7);
    });
  }

  function goToday() {
    setAnchor(today);
    setSelectedDay(today);
  }

  function pickDay(day: Date) {
    setSelectedDay(day);
    setAnchor(day);
  }

  function openNew(at: Date) {
    setEditing({ event: null, initialStart: at });
  }

  function createOnDay(day: Date) {
    pickDay(day);
    openNew(dayAtNine(day));
  }

  async function openEvent(e: CalendarEvent) {
    if (e.recurrence !== null) {
      try {
        const master = await client.getEvent(e.id);
        setEditing({
          event: e,
          initialStart: new Date(e.startsAt),
          occurrenceStart: e.recurrenceId ?? e.startsAt,
          master,
        });
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

  async function saveOccurrence(
    id: string,
    occurrence: string,
    input: EventInput,
  ) {
    await client.overrideOccurrence(id, occurrence, input);
    setEditing(null);
    await reload();
  }

  async function remove(id: string, occurrence?: string) {
    await client.deleteEvent(id, occurrence);
    setEditing(null);
    await reload();
  }

  function dayAtNine(day: Date): Date {
    const d = startOfDay(day);
    d.setHours(9, 0, 0, 0);
    return d;
  }

  const mine = calendars.filter((c) => c.role === "owner");
  const others = calendars.filter((c) => c.role !== "owner");

  const calendarRow = (c: Calendar) => {
    const visible = !hidden.has(c.id);
    const color = colorOf(c.id);
    return (
      <div key={c.id} className={styles.calItem}>
        <button
          type="button"
          className={styles.calToggle}
          onClick={() => toggleCalendar(c.id)}
          aria-pressed={visible}
          aria-label={c.name}
        >
          <span
            className={`${styles.calCheck} ${visible ? styles.calCheckOn : ""}`}
            style={{ ["--cal"]: color } as React.CSSProperties}
          >
            {visible && <Check size={12} strokeWidth={3} />}
          </span>
          <span className={styles.calName}>{c.name}</span>
        </button>
        {c.role !== "owner" && (
          <span className={styles.calShared}>
            {c.role === "editor"
              ? strings.agendaShareEditor
              : strings.agendaShareViewer}
          </span>
        )}
        {c.role === "owner" && (
          <button
            type="button"
            className={styles.calDel}
            onClick={() => setSharing(c)}
            aria-label={strings.agendaShare}
            title={strings.agendaShare}
          >
            <Share2 size={13} />
          </button>
        )}
        {c.role === "owner" && c.kind !== "personal" && (
          <button
            type="button"
            className={styles.calDel}
            onClick={() => void removeCalendar(c.id)}
            aria-label={strings.agendaDeleteCalendar}
            title={strings.agendaDeleteCalendar}
          >
            <Trash2 size={13} />
          </button>
        )}
      </div>
    );
  };

  return (
    <div className={styles.agenda}>
      <aside className={styles.sidebar}>
        <button
          className={styles.newBtn}
          onClick={() => openNew(dayAtNine(selectedDay))}
        >
          <CalendarPlus size={18} />
          {strings.agendaNewEvent}
        </button>
        <MiniMonth anchor={anchor} today={today} onPick={pickDay} />

        <div className={styles.calList}>
          <div className={styles.calListHead}>
            <span>{strings.agendaMyCalendars}</span>
            <button
              type="button"
              className={styles.calAdd}
              onClick={() => void newCalendar()}
              aria-label={strings.agendaNewCalendar}
              title={strings.agendaNewCalendar}
            >
              <Plus size={15} />
            </button>
          </div>
          {mine.map(calendarRow)}
        </div>

        {others.length > 0 && (
          <div className={styles.calList}>
            <div className={styles.calListHead}>
              <span>{strings.agendaOtherCalendars}</span>
            </div>
            {others.map(calendarRow)}
          </div>
        )}
      </aside>

      <section className={styles.main}>
        {/* `keyboard="tab"`, the default: the row carries a heading and a
            spinner between its controls, so the arrow keys a `role="toolbar"`
            promises would have nothing coherent to move between. */}
        <Toolbar
          label={strings.agendaToolbarLabel}
          surface="bar"
          density="compact"
        >
          <Button variant="ghost" size="sm" onClick={goToday}>
            {strings.agendaToday}
          </Button>
          <ToolbarGroup>
            <IconButton
              label={strings.agendaPrev}
              icon={<ChevronLeft size={18} />}
              onClick={() => step(-1)}
            />
            <IconButton
              label={strings.agendaNext}
              icon={<ChevronRight size={18} />}
              onClick={() => step(1)}
            />
          </ToolbarGroup>
          <h1 className={styles.periodLabel}>{label}</h1>
          {loading && <Spinner size={16} />}
          {/* A segmented control: named, so four bare words are announced as
              one choice, and a group so a wrap never splits it in half. */}
          <ToolbarGroup
            label={strings.agendaViewLabel}
            className={styles.viewSwitch}
          >
            {VIEWS.map((v) => (
              <button
                key={v.id}
                type="button"
                className={view === v.id ? styles.viewActive : ""}
                aria-current={view === v.id ? "true" : undefined}
                onClick={() => setView(v.id)}
              >
                {v.label()}
              </button>
            ))}
          </ToolbarGroup>
        </Toolbar>

        <div className={styles.viewport}>
          {view === "month" ? (
            <MonthView
              anchor={anchor}
              today={today}
              selectedDay={selectedDay}
              events={visibleEvents}
              absences={absences}
              colorOf={colorOf}
              onDayClick={createOnDay}
              onEventClick={(e) => void openEvent(e)}
            />
          ) : view === "week" ? (
            <WeekView
              anchor={anchor}
              today={today}
              events={visibleEvents}
              absences={absences}
              onSlotClick={(at) => openNew(at)}
              onEventClick={(e) => void openEvent(e)}
            />
          ) : (
            <AgendaListView
              from={view === "day" ? startOfDay(anchor) : anchor}
              to={view === "day" ? addDays(startOfDay(anchor), 1) : to}
              today={today}
              events={visibleEvents}
              colorOf={colorOf}
              onEventClick={(e) => void openEvent(e)}
            />
          )}
        </div>
      </section>

      <DayPanel
        day={selectedDay}
        today={today}
        events={visibleEvents}
        absences={absences}
        colorOf={colorOf}
        onEventClick={(e) => void openEvent(e)}
      />

      {editing !== null && (
        <EventModal
          event={editing.event}
          master={editing.master}
          initialStart={editing.initialStart}
          occurrenceStart={editing.occurrenceStart}
          calendars={calendars}
          onSave={save}
          onSaveOccurrence={saveOccurrence}
          onDelete={remove}
          onClose={() => setEditing(null)}
        />
      )}

      {sharing !== null && (
        <ShareDialog calendar={sharing} onClose={() => setSharing(null)} />
      )}
    </div>
  );
}
