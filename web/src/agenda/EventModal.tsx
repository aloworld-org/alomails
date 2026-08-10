// Create / edit an event. Times are shown and entered in local time; this
// converts to/from the UTC RFC 3339 the API speaks at save time. All-day events
// use date-only bounds (end is exclusive, so a one-day event ends the next
// midnight).
import { useEffect, useMemo, useState } from "react";
import type { FormEvent } from "react";
import {
  Bell,
  CalendarDays,
  Check,
  Clock,
  FileText,
  Globe,
  MapPin,
  Video,
  Repeat as RepeatIcon,
  Trash2,
  Users,
  X,
} from "lucide-react";

import { strings } from "../i18n";
import { MeetRoom, useMeetApi } from "../meet";
import type { Meeting } from "../meet";
import { Button } from "../ds";
import {
  useJmapClient,
  type Calendar,
  type CalendarEvent,
  type EventInput,
} from "../jmap";
import { addDays, toDateInput, toLocalInput } from "./dates";
import { calendarColorMap } from "./colors";
import styles from "./AgendaModule.module.css";

/** The date part (YYYY-MM-DD) of a `datetime-local` string. */
function dateOf(local: string): string {
  return local.slice(0, 10);
}
/** The time part (HH:mm) of a `datetime-local` string. */
function timeOf(local: string): string {
  return local.slice(11, 16);
}

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
  onSaveOccurrence?: (
    id: string,
    occurrence: string,
    input: EventInput,
  ) => Promise<void>;
  onDelete: (id: string, occurrence?: string) => Promise<void>;
  onClose: () => void;
}

function localFromInput(value: string): Date {
  // `datetime-local` / `date` values are local wall-time; `new Date(local)`
  // parses them in the local zone.
  return new Date(value);
}

type Repeat = "none" | "daily" | "weekly" | "weekdays" | "monthly" | "yearly";

const WEEKDAYS_RRULE = "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR";

/** The dropdown value for an existing RRULE. Recognises the "every weekday"
 *  preset (weekly on Mon–Fri); other BYDAY/BYMONTHDAY rules the engine supports
 *  fall back to their FREQ so editing never silently drops them. */
function repeatOf(rrule: string | null): Repeat {
  const up = (rrule ?? "").toUpperCase();
  if (/FREQ=WEEKLY/.test(up)) {
    const byday = /BYDAY=([^;]+)/.exec(up)?.[1];
    if (byday) {
      const set = new Set(byday.split(",").map((s) => s.trim()));
      if (
        set.size === 5 &&
        ["MO", "TU", "WE", "TH", "FR"].every((d) => set.has(d))
      ) {
        return "weekdays";
      }
    }
  }
  const f = /FREQ=([A-Z]+)/.exec(up)?.[1]?.toLowerCase();
  return f === "daily" || f === "weekly" || f === "monthly" || f === "yearly"
    ? f
    : "none";
}

/** A short label for a guest's RSVP PARTSTAT (organizer's view). */
function rsvpLabel(status: string): string {
  switch (status.toUpperCase()) {
    case "ACCEPTED":
      return strings.agendaRsvpAccepted;
    case "DECLINED":
      return strings.agendaRsvpDeclined;
    case "TENTATIVE":
      return strings.agendaRsvpTentative;
    default:
      return strings.agendaRsvpPending;
  }
}

/** The RRULE for a picker value (`none` → no rule). */
function rruleFor(repeat: Repeat): string | undefined {
  if (repeat === "none") return undefined;
  if (repeat === "weekdays") return WEEKDAYS_RRULE;
  return `FREQ=${repeat.toUpperCase()}`;
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
  const editable = calendars.filter(
    (c) => c.role === "owner" || c.role === "editor",
  );
  const defaultCalendar =
    event?.calendarId ??
    editable.find((c) => c.kind === "personal")?.id ??
    editable[0]?.id ??
    "";
  // An existing event on a view-only shared calendar is shown read-only.
  const readOnly =
    event != null &&
    calendars.find((c) => c.id === event.calendarId)?.role === "viewer";
  const startDate = event ? new Date(event.startsAt) : initialStart;
  const endDate = event
    ? new Date(event.endsAt)
    : new Date(initialStart.getTime() + 3600_000);

  const [summary, setSummary] = useState(event?.summary ?? "");
  const [allDay, setAllDay] = useState(event?.allDay ?? false);
  const [start, setStart] = useState(toLocalInput(startDate));
  const [end, setEnd] = useState(toLocalInput(endDate));
  // All-day: the end date is inclusive in the UI (storage is exclusive).
  const [startDay, setStartDay] = useState(toDateInput(startDate));
  const [endDay, setEndDay] = useState(
    toDateInput(allDay ? addDays(endDate, -1) : startDate),
  );
  const [location, setLocation] = useState(event?.location ?? "");
  // The meeting on this invitation, if it has one. An event that already has a
  // meeting must not grow a second: two links on one invitation puts half the
  // attendees in the wrong call.
  const meet = useMeetApi();
  const [meeting, setMeeting] = useState<Meeting | null>(null);
  const [addingMeeting, setAddingMeeting] = useState(false);
  const [inMeeting, setInMeeting] = useState<string | null>(null);

  useEffect(() => {
    const id = event?.id;
    if (id === undefined) return;
    void meet
      .forEvent(id)
      .then(setMeeting)
      .catch(() => setMeeting(null));
  }, [meet, event?.id]);
  const [guests, setGuests] = useState((event?.attendees ?? []).join(", "));
  const [description, setDescription] = useState(event?.description ?? "");
  const [repeat, setRepeat] = useState<Repeat>(
    repeatOf(event?.recurrence ?? null),
  );
  // "" = no reminder; otherwise minutes-before-start as a string.
  const [reminder, setReminder] = useState<string>(
    event?.reminderMinutes != null ? String(event.reminderMinutes) : "",
  );
  const [calendarId, setCalendarId] = useState(defaultCalendar);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [availability, setAvailability] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const client = useJmapClient();

  const colorMap = useMemo(() => calendarColorMap(calendars), [calendars]);

  // The viewer's timezone, shown read-only so they know what wall-time the
  // fields mean (times are entered locally and stored as UTC).
  const tzLabel = useMemo(() => {
    const zone = Intl.DateTimeFormat().resolvedOptions().timeZone;
    const offMin = -new Date().getTimezoneOffset();
    const sign = offMin >= 0 ? "+" : "-";
    const hh = String(Math.floor(Math.abs(offMin) / 60)).padStart(2, "0");
    const mm = String(Math.abs(offMin) % 60).padStart(2, "0");
    return `(GMT${sign}${hh}:${mm}) ${zone.replace(/_/g, " ")}`;
  }, []);

  function guestList(): string[] {
    return guests
      .split(/[\s,;]+/)
      .map((g) => g.trim())
      .filter((g) => g.includes("@"));
  }

  /** Ask the server who among the guests is busy over the chosen window. */
  async function checkAvailability() {
    const t = readTimes();
    if (t === null) return;
    const people = guestList();
    if (people.length === 0) {
      setAvailability(strings.agendaAvailNoGuests);
      return;
    }
    setChecking(true);
    setAvailability(null);
    try {
      const fb = await client.freeBusy(people, t.startsAt, t.endsAt);
      const s = new Date(t.startsAt).getTime();
      const e = new Date(t.endsAt).getTime();
      const clash = fb
        .filter((p) =>
          p.busy.some(
            (b) =>
              new Date(b.start).getTime() < e && new Date(b.end).getTime() > s,
          ),
        )
        .map((p) => p.email);
      setAvailability(
        clash.length === 0
          ? strings.agendaAvailAllFree
          : strings.agendaAvailBusy(clash.join(", ")),
      );
    } catch {
      setAvailability(strings.agendaAvailError);
    } finally {
      setChecking(false);
    }
  }

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
    const input: EventInput = {
      summary: summary.trim(),
      startsAt,
      endsAt,
      allDay,
    };
    const desc = description.trim();
    if (desc) input.description = desc;
    const loc = location.trim();
    if (loc) input.location = loc;
    const rrule = rruleFor(repeat);
    if (rrule) input.recurrence = rrule;
    if (reminder !== "") input.reminderMinutes = Number(reminder);
    if (calendarId !== "") input.calendarId = calendarId;
    // Guests: address-like tokens only. The server validates and mails each.
    const people = guestList();
    if (people.length > 0) input.attendees = people;
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
      const delta =
        new Date(t.startsAt).getTime() - new Date(event.startsAt).getTime();
      const ms = new Date(master.startsAt).getTime() + delta;
      const me =
        ms +
        (new Date(master.endsAt).getTime() -
          new Date(master.startsAt).getTime());
      input = {
        ...input,
        startsAt: new Date(ms).toISOString(),
        endsAt: new Date(me).toISOString(),
      };
    }
    await run(() => onSave(event?.id ?? null, input));
  }

  /** Override just this occurrence in place (edit "this event" of a series). */
  async function submitThis() {
    if (
      summary.trim() === "" ||
      !onSaveOccurrence ||
      occurrenceStart === undefined ||
      !event
    ) {
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
    event !== null &&
    event.recurrence !== null &&
    occurrenceStart !== undefined;

  return (
    <div
      className={styles.modalScrim}
      role="dialog"
      aria-modal="true"
      onMouseDown={onClose}
    >
      <form
        className={styles.emModal}
        onSubmit={submitSeries}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className={styles.emHead}>
          <span className={styles.emHeadIcon}>
            <CalendarDays size={20} />
          </span>
          <div className={styles.emHeadText}>
            <h2>
              {event
                ? strings.agendaEditEventTitle
                : strings.agendaNewEventTitle}
            </h2>
            <p>
              {event
                ? strings.agendaEditEventSubtitle
                : strings.agendaNewEventSubtitle}
            </p>
          </div>
          <button
            type="button"
            className={styles.emClose}
            onClick={onClose}
            aria-label={strings.agendaCancel}
          >
            <X size={18} />
          </button>
        </div>

        <fieldset disabled={readOnly} className={styles.emBody}>
          <input
            className={styles.emTitle}
            placeholder={strings.agendaEventTitle}
            value={summary}
            onChange={(e) => setSummary(e.target.value)}
            autoCapitalize="sentences"
            required
            autoFocus
          />

          <label className={styles.emToggleRow}>
            <input
              type="checkbox"
              className={styles.emSwitch}
              checked={allDay}
              onChange={(e) => setAllDay(e.target.checked)}
            />
            <span>{strings.agendaAllDay}</span>
          </label>

          <div className={styles.emTwoCol}>
            <div className={styles.emField}>
              <span className={styles.emLabel}>{strings.agendaEventStart}</span>
              <div className={styles.emDateTime}>
                {allDay ? (
                  <span className={styles.emControl}>
                    <CalendarDays size={15} className={styles.emControlIcon} />
                    <input
                      type="date"
                      value={startDay}
                      onChange={(e) => setStartDay(e.target.value)}
                      required
                    />
                  </span>
                ) : (
                  <>
                    <span className={styles.emControl}>
                      <CalendarDays
                        size={15}
                        className={styles.emControlIcon}
                      />
                      <input
                        type="date"
                        value={dateOf(start)}
                        onChange={(e) =>
                          setStart(`${e.target.value}T${timeOf(start)}`)
                        }
                        required
                      />
                    </span>
                    <span className={styles.emControl}>
                      <Clock size={15} className={styles.emControlIcon} />
                      <input
                        type="time"
                        value={timeOf(start)}
                        onChange={(e) =>
                          setStart(`${dateOf(start)}T${e.target.value}`)
                        }
                        required
                      />
                    </span>
                  </>
                )}
              </div>
            </div>
            <div className={styles.emField}>
              <span className={styles.emLabel}>{strings.agendaEventEnd}</span>
              <div className={styles.emDateTime}>
                {allDay ? (
                  <span className={styles.emControl}>
                    <CalendarDays size={15} className={styles.emControlIcon} />
                    <input
                      type="date"
                      value={endDay}
                      onChange={(e) => setEndDay(e.target.value)}
                      required
                    />
                  </span>
                ) : (
                  <>
                    <span className={styles.emControl}>
                      <CalendarDays
                        size={15}
                        className={styles.emControlIcon}
                      />
                      <input
                        type="date"
                        value={dateOf(end)}
                        onChange={(e) =>
                          setEnd(`${e.target.value}T${timeOf(end)}`)
                        }
                        required
                      />
                    </span>
                    <span className={styles.emControl}>
                      <Clock size={15} className={styles.emControlIcon} />
                      <input
                        type="time"
                        value={timeOf(end)}
                        onChange={(e) =>
                          setEnd(`${dateOf(end)}T${e.target.value}`)
                        }
                        required
                      />
                    </span>
                  </>
                )}
              </div>
            </div>
          </div>

          <div className={styles.emTz}>
            <Globe size={15} className={styles.emControlIcon} />
            <span>{tzLabel}</span>
          </div>

          {editable.length > 1 && (
            <div className={styles.emField}>
              <span className={styles.emLabel}>{strings.agendaCalendar}</span>
              <div className={styles.emControl}>
                <span
                  className={styles.emCalDot}
                  style={{ background: colorMap.get(calendarId) ?? "#e76f51" }}
                  aria-hidden
                />
                <select
                  value={calendarId}
                  onChange={(e) => setCalendarId(e.target.value)}
                >
                  {editable.map((c) => (
                    <option key={c.id} value={c.id}>
                      {c.name}
                    </option>
                  ))}
                </select>
              </div>
            </div>
          )}

          <div className={styles.emDivider} />

          <div className={styles.emTwoCol}>
            <div className={styles.emField}>
              <span className={styles.emLabel}>
                <RepeatIcon size={15} /> {strings.agendaRepeat}
              </span>
              <div className={styles.emControl}>
                <select
                  value={repeat}
                  onChange={(e) => setRepeat(e.target.value as Repeat)}
                >
                  <option value="none">{strings.agendaRepeatNone}</option>
                  <option value="daily">{strings.agendaRepeatDaily}</option>
                  <option value="weekly">{strings.agendaRepeatWeekly}</option>
                  <option value="weekdays">
                    {strings.agendaRepeatWeekdays}
                  </option>
                  <option value="monthly">{strings.agendaRepeatMonthly}</option>
                  <option value="yearly">{strings.agendaRepeatYearly}</option>
                </select>
              </div>
            </div>
            <div className={styles.emField}>
              <span className={styles.emLabel}>
                <Bell size={15} /> {strings.agendaReminder}
              </span>
              <div className={styles.emControl}>
                <select
                  value={reminder}
                  onChange={(e) => setReminder(e.target.value)}
                >
                  <option value="">{strings.agendaReminderNone}</option>
                  <option value="0">{strings.agendaReminderAtStart}</option>
                  <option value="5">{strings.agendaReminder5}</option>
                  <option value="10">{strings.agendaReminder10}</option>
                  <option value="15">{strings.agendaReminder15}</option>
                  <option value="30">{strings.agendaReminder30}</option>
                  <option value="60">{strings.agendaReminder60}</option>
                  <option value="1440">{strings.agendaReminder1Day}</option>
                </select>
              </div>
            </div>
          </div>

          <div className={styles.emTwoCol}>
            <div className={styles.emField}>
              <span className={styles.emLabel}>
                <MapPin size={15} /> {strings.agendaEventLocation}
              </span>
              <div className={styles.emControl}>
                <input
                  value={location}
                  onChange={(e) => setLocation(e.target.value)}
                  placeholder={strings.agendaLocationPlaceholder}
                />
                <MapPin size={15} className={styles.emControlTrail} />
              </div>
            </div>
            {inMeeting !== null && (
              <MeetRoom
                meetingId={inMeeting}
                onLeft={() => setInMeeting(null)}
              />
            )}
            {/* A meeting belongs to the invitation, so this is where it is
                added and where everyone invited finds it. Offered only once
                the event exists — an unsaved event has no id to attach to. */}
            {event?.id !== undefined && (
              <div className={styles.emField}>
                <span className={styles.emLabel}>
                  <Video size={15} /> {strings.meetTitle}
                </span>
                <div className={styles.emControl}>
                  {meeting !== null ? (
                    <button
                      type="button"
                      className={styles.emMeetJoin}
                      onClick={() => setInMeeting(meeting.id)}
                    >
                      <Video size={15} /> {strings.meetJoin}
                    </button>
                  ) : (
                    <button
                      type="button"
                      className={styles.emMeetAdd}
                      disabled={addingMeeting}
                      onClick={() => {
                        setAddingMeeting(true);
                        void meet
                          .start({ event: event.id, title: summary })
                          .then(setMeeting)
                          .catch(() => setMeeting(null))
                          .finally(() => setAddingMeeting(false));
                      }}
                    >
                      <Video size={15} /> {strings.meetAddToEvent}
                    </button>
                  )}
                </div>
              </div>
            )}
            <div className={styles.emField}>
              <span className={styles.emLabel}>
                <Users size={15} /> {strings.agendaEventGuests}
              </span>
              <div className={styles.emControl}>
                <input
                  value={guests}
                  onChange={(e) => setGuests(e.target.value)}
                  placeholder={strings.agendaGuestsPlaceholder}
                  inputMode="email"
                  autoCapitalize="none"
                  autoCorrect="off"
                  spellCheck={false}
                />
              </div>
              <small className={styles.fieldHint}>
                {strings.agendaGuestsHint}
              </small>
              <div className={styles.availabilityRow}>
                <button
                  type="button"
                  className={styles.emAvailBtn}
                  onClick={() => void checkAvailability()}
                  disabled={checking}
                >
                  <CalendarDays size={15} />
                  {checking
                    ? strings.agendaAvailChecking
                    : strings.agendaCheckAvailability}
                </button>
                {availability !== null && (
                  <small className={styles.fieldHint}>{availability}</small>
                )}
              </div>
              {event?.attendeeStatus && event.attendeeStatus.length > 0 && (
                <small className={styles.fieldHint}>
                  {event.attendeeStatus
                    .map((a) => `${a.email} — ${rsvpLabel(a.status)}`)
                    .join(" · ")}
                </small>
              )}
            </div>
          </div>

          <div className={styles.emDivider} />

          <div className={styles.emField}>
            <span className={styles.emLabel}>
              <FileText size={15} /> {strings.agendaEventDescription}
            </span>
            <textarea
              className={styles.emTextarea}
              rows={3}
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder={strings.agendaDescriptionPlaceholder}
            />
          </div>
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

        <div className={styles.emFooter}>
          {!readOnly && event !== null && !recurringOccurrence && (
            <button
              type="button"
              className={styles.deleteBtn}
              onClick={() => void remove()}
              disabled={busy}
            >
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
            <button
              type="button"
              className={styles.emCancel}
              onClick={onClose}
              disabled={busy}
            >
              {readOnly ? strings.agendaClose : strings.agendaCancel}
            </button>
            {!readOnly && recurringOccurrence && onSaveOccurrence && (
              <button
                type="button"
                className={styles.emCancel}
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
                  <Check size={16} />{" "}
                  {event ? strings.agendaSave : strings.agendaCreateEvent}
                </Button>
              ))}
          </div>
        </div>
      </form>
    </div>
  );
}
