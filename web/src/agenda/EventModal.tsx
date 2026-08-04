// Create / edit an event. Times are shown and entered in local time; this
// converts to/from the UTC RFC 3339 the API speaks at save time. All-day events
// use date-only bounds (end is exclusive, so a one-day event ends the next
// midnight).
import { useState } from "react";
import type { FormEvent } from "react";
import { MapPin, Trash2, Users, X } from "lucide-react";

import { strings } from "../i18n";
import { Button } from "../ds";
import type { Calendar, CalendarEvent, EventInput } from "../jmap";
import { addDays, toDateInput, toLocalInput } from "./dates";
import styles from "./AgendaModule.module.css";

interface Props {
  /** The event being edited, or `null` for a new one. */
  event: CalendarEvent | null;
  /** For a new event, the local start the user clicked (defaults applied). */
  initialStart: Date;
  /** For a recurring event, the RFC 3339 ORIGINAL slot of the clicked occurrence
   *  — enables "this event" (skip or override just that instance). */
  occurrenceStart?: string | undefined;
  /** The stored series master (recurring edits only); its base time anchors a
   *  whole-series ("all events") edit. */
  master?: CalendarEvent | undefined;
  /** The calendars the event can be placed on. */
  calendars: Calendar[];
  onSave: (id: string | null, input: EventInput) => Promise<void>;
  /** Override just one occurrence in place (edit this instance of a series). */
  onSaveOccurrence?: (id: string, occurrence: string, input: EventInput) => Promise<void>;
  onDelete: (id: string, occurrence?: string) => Promise<void>;
  onClose: () => void;
}

function localFromInput(value: string): Date {
  // `datetime-local` / `date` values are local wall-time; `new Date(local)`
  // parses them in the local zone.
  return new Date(value);
}

type Repeat = "none" | "daily" | "weekly" | "monthly" | "yearly";

/** The dropdown value for an existing RRULE (by FREQ; extra params like
 *  INTERVAL/BYDAY aren't surfaced in this simple picker). */
function repeatOf(rrule: string | null): Repeat {
  const m = /FREQ=([A-Z]+)/i.exec(rrule ?? "");
  const f = m?.[1]?.toLowerCase();
  return f === "daily" || f === "weekly" || f === "monthly" || f === "yearly" ? f : "none";
}

export function EventModal({
  event,
  master,
  initialStart,
  occurrenceStart,
  calendars,
  onSave,
  onSaveOccurrence,
  onDelete,
  onClose,
}: Props) {
  // Only calendars the viewer can write to may host a new or moved event.
  const editable = calendars.filter((c) => c.role === "owner" || c.role === "editor");
  const defaultCalendar =
    event?.calendarId ?? editable.find((c) => c.kind === "personal")?.id ?? editable[0]?.id ?? "";
  // An existing event on a view-only shared calendar is shown read-only.
  const readOnly =
    event != null && calendars.find((c) => c.id === event.calendarId)?.role === "viewer";
  const startDate = event ? new Date(event.startsAt) : initialStart;
  const endDate = event ? new Date(event.endsAt) : new Date(initialStart.getTime() + 3600_000);

  const [summary, setSummary] = useState(event?.summary ?? "");
  const [allDay, setAllDay] = useState(event?.allDay ?? false);
  const [start, setStart] = useState(toLocalInput(startDate));
  const [end, setEnd] = useState(toLocalInput(endDate));
  // All-day: the end date is inclusive in the UI (storage is exclusive).
  const [startDay, setStartDay] = useState(toDateInput(startDate));
  const [endDay, setEndDay] = useState(toDateInput(allDay ? addDays(endDate, -1) : startDate));
  const [location, setLocation] = useState(event?.location ?? "");
  const [guests, setGuests] = useState((event?.attendees ?? []).join(", "));
  const [description, setDescription] = useState(event?.description ?? "");
  const [repeat, setRepeat] = useState<Repeat>(repeatOf(event?.recurrence ?? null));
  const [calendarId, setCalendarId] = useState(defaultCalendar);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  /** The form's start/end as RFC 3339 UTC, or null if the range is invalid. */
  function readTimes(): { startsAt: string; endsAt: string } | null {
    if (allDay) {
      const s = localFromInput(`${startDay}T00:00`);
      const eExclusive = addDays(localFromInput(`${endDay}T00:00`), 1); // exclusive end
      if (eExclusive <= s) {
        setError(strings.agendaEndBeforeStart);
        return null;
      }
      return { startsAt: s.toISOString(), endsAt: eExclusive.toISOString() };
    }
    const s = localFromInput(start);
    const en = localFromInput(end);
    if (en < s) {
      setError(strings.agendaEndBeforeStart);
      return null;
    }
    return { startsAt: s.toISOString(), endsAt: en.toISOString() };
  }

  /** Assemble the writable event fields, omitting empty optionals. */
  function inputFrom(startsAt: string, endsAt: string): EventInput {
    const input: EventInput = { summary: summary.trim(), startsAt, endsAt, allDay };
    const desc = description.trim();
    if (desc) input.description = desc;
    const loc = location.trim();
    if (loc) input.location = loc;
    if (repeat !== "none") input.recurrence = `FREQ=${repeat.toUpperCase()}`;
    if (calendarId !== "") input.calendarId = calendarId;
    // Guests: split on commas/semicolons/whitespace, keep anything address-like.
    // The server validates and mails each an invitation.
    const guestList = guests
      .split(/[\s,;]+/)
      .map((g) => g.trim())
      .filter((g) => g.includes("@"));
    if (guestList.length > 0) input.attendees = guestList;
    return input;
  }

  async function run(fn: () => Promise<void>) {
    setError(null);
    setBusy(true);
    try {
      await fn();
    } catch {
      setError(strings.agendaSaveError);
      setBusy(false);
    }
  }

  /** Save the whole event/series (new, one-off, or "all events"). When editing
   *  a recurring occurrence, the master is shifted by however much the user
   *  moved this instance, so the series keeps its cadence. */
  async function submitSeries(e?: FormEvent) {
    e?.preventDefault();
    if (summary.trim() === "") return;
    const t = readTimes();
    if (t === null) return;
    let input = inputFrom(t.startsAt, t.endsAt);
    if (recurringOccurrence && master && event) {
      const delta = new Date(t.startsAt).getTime() - new Date(event.startsAt).getTime();
      const ms = new Date(master.startsAt).getTime() + delta;
      const me = ms + (new Date(master.endsAt).getTime() - new Date(master.startsAt).getTime());
      input = { ...input, startsAt: new Date(ms).toISOString(), endsAt: new Date(me).toISOString() };
    }
    await run(() => onSave(event?.id ?? null, input));
  }

  /** Override just this occurrence in place (edit "this event" of a series). */
  async function submitThis() {
    if (summary.trim() === "" || !onSaveOccurrence || occurrenceStart === undefined || !event) {
      return;
    }
    const t = readTimes();
    if (t === null) return;
    const input = inputFrom(t.startsAt, t.endsAt);
    await run(() => onSaveOccurrence(event.id, occurrenceStart, input));
  }

  async function remove(occurrence?: string) {
    if (event === null) return;
    setBusy(true);
    try {
      await onDelete(event.id, occurrence);
    } catch {
      setError(strings.agendaSaveError);
      setBusy(false);
    }
  }

  // A recurring event opened from a specific instance offers "this one" vs the
  // whole series; a one-off just deletes.
  const recurringOccurrence =
    event !== null && event.recurrence !== null && occurrenceStart !== undefined;

  return (
    <div className={styles.modalScrim} role="dialog" aria-modal="true" onMouseDown={onClose}>
      <form className={styles.modal} onSubmit={submitSeries} onMouseDown={(e) => e.stopPropagation()}>
        <div className={styles.modalHead}>
          <h2>{event ? strings.agendaEditEventTitle : strings.agendaNewEventTitle}</h2>
          <button type="button" className={styles.iconBtn} onClick={onClose} aria-label={strings.agendaCancel}>
            <X size={18} />
          </button>
        </div>

        {/* A disabled fieldset makes every control read-only in one place; it
            lays out transparently (display:contents) so nothing shifts. */}
        <fieldset disabled={readOnly} style={{ display: "contents" }}>
        <input
          className={styles.titleInput}
          placeholder={strings.agendaEventTitle}
          value={summary}
          onChange={(e) => setSummary(e.target.value)}
          autoCapitalize="sentences"
          required
          autoFocus
        />

        <label className={styles.allDayRow}>
          <input type="checkbox" checked={allDay} onChange={(e) => setAllDay(e.target.checked)} />
          <span>{strings.agendaAllDay}</span>
        </label>

        {allDay ? (
          <div className={styles.timeRow}>
            <label className={styles.field}>
              <span>{strings.agendaEventStart}</span>
              <input type="date" value={startDay} onChange={(e) => setStartDay(e.target.value)} required />
            </label>
            <label className={styles.field}>
              <span>{strings.agendaEventEnd}</span>
              <input type="date" value={endDay} onChange={(e) => setEndDay(e.target.value)} required />
            </label>
          </div>
        ) : (
          <div className={styles.timeRow}>
            <label className={styles.field}>
              <span>{strings.agendaEventStart}</span>
              <input type="datetime-local" value={start} onChange={(e) => setStart(e.target.value)} required />
            </label>
            <label className={styles.field}>
              <span>{strings.agendaEventEnd}</span>
              <input type="datetime-local" value={end} onChange={(e) => setEnd(e.target.value)} required />
            </label>
          </div>
        )}

        {editable.length > 1 && (
          <label className={styles.field}>
            <span>{strings.agendaCalendar}</span>
            <select value={calendarId} onChange={(e) => setCalendarId(e.target.value)}>
              {editable.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </select>
          </label>
        )}

        <label className={styles.field}>
          <span>{strings.agendaRepeat}</span>
          <select value={repeat} onChange={(e) => setRepeat(e.target.value as Repeat)}>
            <option value="none">{strings.agendaRepeatNone}</option>
            <option value="daily">{strings.agendaRepeatDaily}</option>
            <option value="weekly">{strings.agendaRepeatWeekly}</option>
            <option value="monthly">{strings.agendaRepeatMonthly}</option>
            <option value="yearly">{strings.agendaRepeatYearly}</option>
          </select>
        </label>

        <label className={styles.field}>
          <span>
            <MapPin size={13} /> {strings.agendaEventLocation}
          </span>
          <input value={location} onChange={(e) => setLocation(e.target.value)} />
        </label>

        <label className={styles.field}>
          <span>
            <Users size={13} /> {strings.agendaEventGuests}
          </span>
          <input
            value={guests}
            onChange={(e) => setGuests(e.target.value)}
            placeholder={strings.agendaGuestsPlaceholder}
            inputMode="email"
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
          />
          <small className={styles.fieldHint}>{strings.agendaGuestsHint}</small>
        </label>

        <label className={styles.field}>
          <span>{strings.agendaEventDescription}</span>
          <textarea rows={3} value={description} onChange={(e) => setDescription(e.target.value)} />
        </label>
        </fieldset>

        {readOnly && (
          <p className={styles.fieldHint} role="note">
            {strings.agendaReadOnly}
          </p>
        )}

        {error !== null && (
          <p className={styles.modalError} role="alert">
            {error}
          </p>
        )}

        <div className={styles.modalActions}>
          {!readOnly && event !== null && !recurringOccurrence && (
            <button type="button" className={styles.deleteBtn} onClick={() => void remove()} disabled={busy}>
              <Trash2 size={15} /> {strings.agendaDelete}
            </button>
          )}
          {!readOnly && recurringOccurrence && (
            <div className={styles.deleteChoice}>
              <button
                type="button"
                className={styles.deleteBtn}
                onClick={() => void remove(occurrenceStart)}
                disabled={busy}
              >
                <Trash2 size={15} /> {strings.agendaDeleteThis}
              </button>
              <button
                type="button"
                className={styles.deleteBtn}
                onClick={() => void remove()}
                disabled={busy}
              >
                {strings.agendaDeleteAll}
              </button>
            </div>
          )}
          <div className={styles.modalActionsRight}>
            <button type="button" className={styles.linkBtn} onClick={onClose} disabled={busy}>
              {readOnly ? strings.agendaClose : strings.agendaCancel}
            </button>
            {!readOnly && recurringOccurrence && onSaveOccurrence && (
              <button
                type="button"
                className={styles.linkBtn}
                onClick={() => void submitThis()}
                disabled={busy || summary.trim() === ""}
              >
                {strings.agendaSaveThis}
              </button>
            )}
            {!readOnly &&
              (recurringOccurrence ? (
                <Button
                  type="button"
                  onClick={() => void submitSeries()}
                  disabled={busy || summary.trim() === ""}
                >
                  {strings.agendaSaveAll}
                </Button>
              ) : (
                <Button type="submit" disabled={busy || summary.trim() === ""}>
                  {strings.agendaSave}
                </Button>
              ))}
          </div>
        </div>
      </form>
    </div>
  );
}
