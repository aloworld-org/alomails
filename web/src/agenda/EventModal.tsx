// Create / edit an event. Times are shown and entered in local time; this
// converts to/from the UTC RFC 3339 the API speaks at save time. All-day events
// use date-only bounds (end is exclusive, so a one-day event ends the next
// midnight).
import { useState } from "react";
import type { FormEvent } from "react";
import { MapPin, Trash2, X } from "lucide-react";

import { strings } from "../i18n";
import { Button } from "../ds";
import type { CalendarEvent, EventInput } from "../jmap";
import { addDays, toDateInput, toLocalInput } from "./dates";
import styles from "./AgendaModule.module.css";

interface Props {
  /** The event being edited, or `null` for a new one. */
  event: CalendarEvent | null;
  /** For a new event, the local start the user clicked (defaults applied). */
  initialStart: Date;
  onSave: (id: string | null, input: EventInput) => Promise<void>;
  onDelete: (id: string) => Promise<void>;
  onClose: () => void;
}

function localFromInput(value: string): Date {
  // `datetime-local` / `date` values are local wall-time; `new Date(local)`
  // parses them in the local zone.
  return new Date(value);
}

export function EventModal({ event, initialStart, onSave, onDelete, onClose }: Props) {
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
  const [description, setDescription] = useState(event?.description ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (summary.trim() === "") return;
    let startsAt: string;
    let endsAt: string;
    if (allDay) {
      const s = localFromInput(`${startDay}T00:00`);
      const eInclusive = localFromInput(`${endDay}T00:00`);
      const eExclusive = addDays(eInclusive, 1); // exclusive end
      if (eExclusive <= s) {
        setError(strings.agendaEndBeforeStart);
        return;
      }
      startsAt = s.toISOString();
      endsAt = eExclusive.toISOString();
    } else {
      const s = localFromInput(start);
      const en = localFromInput(end);
      if (en < s) {
        setError(strings.agendaEndBeforeStart);
        return;
      }
      startsAt = s.toISOString();
      endsAt = en.toISOString();
    }
    setError(null);
    setBusy(true);
    // Omit empty optional fields (exactOptionalPropertyTypes: no `undefined`).
    const input: EventInput = { summary: summary.trim(), startsAt, endsAt, allDay };
    const desc = description.trim();
    if (desc) input.description = desc;
    const loc = location.trim();
    if (loc) input.location = loc;
    try {
      await onSave(event?.id ?? null, input);
    } catch {
      setError(strings.agendaSaveError);
      setBusy(false);
    }
  }

  async function remove() {
    if (event === null) return;
    setBusy(true);
    try {
      await onDelete(event.id);
    } catch {
      setError(strings.agendaSaveError);
      setBusy(false);
    }
  }

  return (
    <div className={styles.modalScrim} role="dialog" aria-modal="true" onMouseDown={onClose}>
      <form className={styles.modal} onSubmit={submit} onMouseDown={(e) => e.stopPropagation()}>
        <div className={styles.modalHead}>
          <h2>{event ? strings.agendaEditEventTitle : strings.agendaNewEventTitle}</h2>
          <button type="button" className={styles.iconBtn} onClick={onClose} aria-label={strings.agendaCancel}>
            <X size={18} />
          </button>
        </div>

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

        <label className={styles.field}>
          <span>
            <MapPin size={13} /> {strings.agendaEventLocation}
          </span>
          <input value={location} onChange={(e) => setLocation(e.target.value)} />
        </label>

        <label className={styles.field}>
          <span>{strings.agendaEventDescription}</span>
          <textarea rows={3} value={description} onChange={(e) => setDescription(e.target.value)} />
        </label>

        {error !== null && (
          <p className={styles.modalError} role="alert">
            {error}
          </p>
        )}

        <div className={styles.modalActions}>
          {event !== null && (
            <button type="button" className={styles.deleteBtn} onClick={() => void remove()} disabled={busy}>
              <Trash2 size={15} /> {strings.agendaDelete}
            </button>
          )}
          <div className={styles.modalActionsRight}>
            <button type="button" className={styles.linkBtn} onClick={onClose} disabled={busy}>
              {strings.agendaCancel}
            </button>
            <Button type="submit" disabled={busy || summary.trim() === ""}>
              {strings.agendaSave}
            </Button>
          </div>
        </div>
      </form>
    </div>
  );
}
