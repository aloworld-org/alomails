// What a visitor may book on this website, in one screen (ADR 0036, S2.13c):
// the site's bookable services, the Agenda calendar each one is attached to,
// the week it is offered in, and the questions asked when it is taken.
//
// Three things the screen has to keep saying out loud, because a visitor would
// otherwise be the one to find them out: a service is booked into a REAL
// calendar and its appointments are managed there, not here; a calendar the
// account can no longer reach leaves the service offering nothing; and a free
// calendar is not an invitation — the opening hours below are what is offered,
// and the calendar can only ever take times away.
//
// Nothing is validated here. Lengths, overlapping windows, time-zone names and
// question keys are ruled on by the store, and its refusal — a sentence naming
// the rule — is what the screen shows.
import { useCallback, useEffect, useState } from "react";
import {
  ArrowLeft,
  CalendarClock,
  CalendarDays,
  Plus,
  Trash2,
} from "lucide-react";
import { Link, useParams } from "react-router-dom";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import {
  WEEKDAYS,
  blankBookingField,
  blankWindow,
  bookingDraft,
  emptyBookingDraft,
  suggestFieldKey,
  timeMinutes,
  timeValue,
  weekdayName,
  windowLabel,
} from "./bookingSchedule";
import { EmptyState, ErrorBanner } from "./parts";
import type {
  SiteAvailabilitySource,
  SiteBooking,
  SiteBookingDraft,
  SiteBookingField,
  SiteBookingFieldKind,
  SiteBookingWindow,
} from "./types";

const styles = {
  page: "mx-auto flex w-full max-w-[90rem] flex-col gap-5 px-4 py-5 sm:px-6 lg:px-8",
  header:
    "flex flex-col gap-4 rounded-2xl border border-subtle bg-surface px-5 py-4 shadow-sm sm:flex-row sm:items-center sm:justify-between",
  backLink:
    "inline-flex min-h-10 items-center gap-2 rounded-xl px-3 text-sm font-semibold text-secondary no-underline transition-colors hover:bg-app hover:text-primary",
  title: "text-xl font-bold tracking-tight text-primary",
  collectionPageHint: "mt-1 max-w-2xl text-sm leading-6 text-secondary",
  collectionLoading:
    "flex min-h-[24rem] items-center justify-center gap-3 rounded-2xl border border-subtle bg-surface text-sm text-secondary",
  catalogWorkspace:
    "grid min-h-[42rem] overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm lg:grid-cols-[19rem_minmax(0,1fr)]",
  catalogList: "border-b border-subtle bg-app/35 p-2 lg:border-b-0 lg:border-r",
  collectionListEmpty:
    "flex flex-col items-center gap-2 px-5 py-10 text-center text-sm text-secondary [&>svg]:h-8 [&>svg]:w-8 [&>svg]:text-muted [&>strong]:text-base [&>strong]:text-primary",
  collectionListItem:
    "mb-1 flex w-full items-start gap-3 rounded-xl border border-transparent px-4 py-3 text-left text-primary transition-colors hover:border-subtle hover:bg-surface [&>svg]:mt-0.5 [&>svg]:h-5 [&>svg]:w-5 [&>svg]:shrink-0 [&>span]:min-w-0 [&_strong]:block [&_strong]:truncate [&_strong]:text-sm [&_small]:mt-1 [&_small]:block [&_small]:truncate [&_small]:text-xs [&_small]:text-secondary",
  collectionListItemActive: "!border-accent/25 !bg-accent-soft shadow-sm",
  catalogEditor:
    "grid min-w-0 gap-5 p-4 xl:grid-cols-[minmax(0,1fr)_20rem] xl:p-6",
  catalogPanel: "min-w-0 space-y-6",
  catalogItemsPanel:
    "h-fit min-w-0 rounded-2xl border border-subtle bg-app/35 p-5 xl:sticky xl:top-5",
  collectionPanelHead:
    "flex items-start justify-between gap-4 border-b border-subtle pb-4 [&_h2]:text-lg [&_h2]:font-bold [&_h2]:text-primary [&_p]:mt-1 [&_p]:text-sm [&_p]:leading-6 [&_p]:text-secondary",
  collectionSourceFields: "grid gap-4 sm:grid-cols-2",
  bookingDescription: "block",
  input:
    "mt-2 min-h-11 w-full rounded-xl border border-subtle bg-surface px-3.5 py-2.5 text-sm text-primary outline-none transition-shadow placeholder:text-muted focus:border-accent focus:ring-2 focus:ring-accent/15",
  textarea: "min-h-24 resize-y",
  bookingCalendar: "rounded-2xl border border-subtle bg-app/40 p-4",
  hint: "mt-2 text-sm leading-6 text-secondary",
  publishError: "mt-2 rounded-xl bg-danger-soft px-3 py-2 text-sm text-danger",
  liveLink:
    "mt-3 inline-flex min-h-10 items-center gap-2 rounded-xl bg-neutral-soft px-3.5 text-sm font-semibold text-primary no-underline hover:bg-neutral-soft/75",
  bookingNumbers: "grid gap-4 sm:grid-cols-2 xl:grid-cols-3",
  mono: "font-mono",
  catalogGroups:
    "rounded-2xl border border-subtle p-4 [&>h3]:font-semibold [&>h3]:text-primary [&>p]:mt-1 [&>p]:text-sm [&>p]:text-secondary",
  catalogGroupRows: "mt-4 space-y-3",
  bookingHoursRow:
    "grid items-center gap-2 sm:grid-cols-[minmax(8rem,1fr)_8rem_8rem_auto]",
  bookingQuestionRow:
    "grid items-center gap-2 rounded-xl bg-app/40 p-3 sm:grid-cols-2 xl:grid-cols-[minmax(8rem,1fr)_minmax(7rem,.7fr)_9rem_auto_auto]",
  bookingRequired:
    "flex min-h-11 items-center gap-2 px-2 text-sm text-secondary",
  catalogOrdersToggle:
    "flex cursor-pointer items-start gap-3 rounded-2xl border border-subtle bg-app/40 p-4 [&_input]:mt-1 [&_span]:min-w-0 [&_strong]:block [&_strong]:text-sm [&_strong]:text-primary [&_small]:mt-1 [&_small]:block [&_small]:text-sm [&_small]:leading-6 [&_small]:text-secondary",
  collectionActions:
    "flex flex-col-reverse gap-3 border-t border-subtle pt-5 sm:flex-row sm:items-center sm:justify-between",
  collectionDisconnectGroup: "flex items-center gap-3 text-sm text-danger",
  bookingPreview:
    "mt-5 space-y-3 text-sm text-secondary [&>strong]:block [&>strong]:text-lg [&>strong]:text-primary",
  bookingPreviewHours:
    "space-y-2 rounded-xl bg-surface p-3 text-sm text-primary",
} as const;

/** The calendar a new service starts on: the first one appointments can
 *  actually be written into, and failing that the first one there is. Choosing
 *  a read-only share for them rather than nothing means pressing Create earns
 *  the server's sentence naming the rule, instead of a button that is disabled
 *  for a reason the screen never says. */
function defaultCalendar(sources: SiteAvailabilitySource[]): string {
  return (sources.find((source) => source.writable) ?? sources[0])?.id ?? "";
}

/** The four kinds of extra question, in the order they are offered. */
const FIELD_KINDS: readonly SiteBookingFieldKind[] = [
  "text",
  "long_text",
  "phone",
  "choice",
];

function fieldKindLabel(kind: SiteBookingFieldKind): string {
  switch (kind) {
    case "text":
      return strings.sitesBookingQuestionText;
    case "long_text":
      return strings.sitesBookingQuestionLongText;
    case "phone":
      return strings.sitesBookingQuestionPhone;
    case "choice":
      return strings.sitesBookingQuestionChoice;
  }
}

export function BookingsView() {
  const { siteId = "" } = useParams();
  const api = useSitesApi();
  const [bookings, setBookings] = useState<SiteBooking[]>([]);
  const [sources, setSources] = useState<SiteAvailabilitySource[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draft, setDraft] = useState<SiteBookingDraft>(() =>
    emptyBookingDraft(""),
  );
  const [creating, setCreating] = useState(false);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [deleteArmed, setDeleteArmed] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      // Both reads together: which calendars exist is part of drawing the very
      // first row of this screen, not a detail fetched once a service is
      // selected.
      const [stored, calendars] = await Promise.all([
        api.bookings(siteId),
        api.bookingSources(siteId),
      ]);
      setBookings(stored);
      setSources(calendars);
      const first = stored[0];
      setSelectedId(first?.id ?? null);
      setDraft(
        first === undefined
          ? emptyBookingDraft(defaultCalendar(calendars))
          : bookingDraft(first),
      );
      setCreating(first === undefined);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesBookingsLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, siteId]);

  useEffect(() => {
    void load();
  }, [load]);

  const selected =
    bookings.find((booking) => booking.id === selectedId) ?? null;
  /** The calendar the draft names, as the account can see it now. Absent means
   *  it has been deleted or unshared since — the one state that silently stops
   *  a published page taking bookings, so the screen never leaves it unsaid. */
  const boundSource = sources.find((source) => source.id === draft.calendarId);

  function select(booking: SiteBooking) {
    setSelectedId(booking.id);
    setDraft(bookingDraft(booking));
    setCreating(false);
    setDeleteArmed(false);
    setError(null);
  }

  function startCreate() {
    setSelectedId(null);
    setDraft(emptyBookingDraft(defaultCalendar(sources)));
    setCreating(true);
    setDeleteArmed(false);
    setError(null);
  }

  function edit(change: Partial<SiteBookingDraft>) {
    setDraft((current) => ({ ...current, ...change }));
  }

  function editWindow(index: number, change: Partial<SiteBookingWindow>) {
    setDraft((current) => ({
      ...current,
      hours: current.hours.map((window, at) =>
        at === index ? { ...window, ...change } : window,
      ),
    }));
  }

  function editField(index: number, change: Partial<SiteBookingField>) {
    setDraft((current) => ({
      ...current,
      fields: current.fields.map((field, at) =>
        at === index ? { ...field, ...change } : field,
      ),
    }));
  }

  async function save() {
    if (draft.name.trim() === "" || draft.calendarId === "") return;
    setBusy(true);
    setError(null);
    try {
      const stored =
        creating || selectedId === null
          ? await api.createBooking(siteId, draft)
          : await api.updateBooking(siteId, selectedId, draft);
      setBookings((current) =>
        current.some((booking) => booking.id === stored.id)
          ? current.map((booking) =>
              booking.id === stored.id ? stored : booking,
            )
          : [...current, stored],
      );
      setSelectedId(stored.id);
      setDraft(bookingDraft(stored));
      setCreating(false);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesBookingSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  async function remove() {
    if (selectedId === null) return;
    if (!deleteArmed) {
      setDeleteArmed(true);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await api.deleteBooking(siteId, selectedId);
      const remaining = bookings.filter((booking) => booking.id !== selectedId);
      setBookings(remaining);
      const next = remaining[0];
      setSelectedId(next?.id ?? null);
      setDraft(
        next === undefined
          ? emptyBookingDraft(defaultCalendar(sources))
          : bookingDraft(next),
      );
      setCreating(next === undefined);
      setDeleteArmed(false);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesBookingDeleteFailed));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <Link to=".." relative="path" className={styles.backLink}>
          <ArrowLeft size="var(--icon-size-inline)" aria-hidden="true" />
          {strings.sitesBack}
        </Link>
        <div>
          <h1 className={styles.title}>{strings.sitesBookings}</h1>
          <p className={styles.collectionPageHint}>
            {strings.sitesBookingsHint}
          </p>
        </div>
        {!loading && bookings.length > 0 && sources.length > 0 && (
          <Button
            icon={<Plus size="var(--icon-size-inline)" />}
            onClick={startCreate}
          >
            {strings.sitesNewBooking}
          </Button>
        )}
      </header>

      {error !== null && <ErrorBanner message={error} />}

      {loading ? (
        <div className={styles.collectionLoading} role="status">
          <Spinner size={20} />
          <span>{strings.sitesBookingsLoading}</span>
        </div>
      ) : sources.length === 0 ? (
        // The dependency, stated before anything else: Sites does not own a
        // calendar and cannot make one. Nothing on this screen is usable until
        // Agenda has one, so the screen says so instead of offering a picker
        // with nothing in it.
        <EmptyState
          Icon={CalendarDays}
          title={strings.sitesBookingNoCalendarTitle}
          body={strings.sitesBookingNoCalendarBody}
        />
      ) : bookings.length === 0 && !creating ? (
        <EmptyState
          Icon={CalendarClock}
          title={strings.sitesBookingNoneTitle}
          body={strings.sitesBookingNoneBody}
          cta={strings.sitesNewBooking}
          onCta={startCreate}
        />
      ) : (
        <div className={styles.catalogWorkspace}>
          <aside
            className={styles.catalogList}
            aria-label={strings.sitesBookings}
          >
            {/* The first visit lands here with the create form already open, so
                the list says what a bookable service IS rather than showing an
                empty column beside a form nobody asked for. */}
            {bookings.length === 0 && (
              <div className={styles.collectionListEmpty}>
                <CalendarClock aria-hidden="true" />
                <strong>{strings.sitesBookingNoneTitle}</strong>
                <span>{strings.sitesBookingNoneBody}</span>
              </div>
            )}
            {bookings.map((booking) => (
              <button
                key={booking.id}
                type="button"
                className={`${styles.collectionListItem} ${
                  booking.id === selectedId
                    ? styles.collectionListItemActive
                    : ""
                }`}
                aria-pressed={booking.id === selectedId}
                onClick={() => select(booking)}
              >
                <CalendarClock aria-hidden="true" />
                <span>
                  <strong>{booking.name}</strong>
                  <small>
                    {strings.sitesBookingMinutes(booking.durationMinutes)} ·{" "}
                    {booking.calendar === null
                      ? strings.sitesBookingCalendarGone
                      : booking.calendar.name}
                    {!booking.active && ` · ${strings.sitesBookingOff}`}
                  </small>
                </span>
              </button>
            ))}
          </aside>

          <div className={styles.catalogEditor}>
            <section
              className={styles.catalogPanel}
              aria-labelledby="booking-settings-title"
            >
              <div className={styles.collectionPanelHead}>
                <div>
                  <h2 id="booking-settings-title">
                    {creating || selected === null
                      ? strings.sitesNewBooking
                      : strings.sitesBookingSettings}
                  </h2>
                  <p>{strings.sitesBookingSettingsHint}</p>
                </div>
              </div>

              <div className={styles.collectionSourceFields}>
                <label>
                  <span>{strings.sitesBookingName}</span>
                  <input
                    className={styles.input}
                    value={draft.name}
                    onChange={(event) => edit({ name: event.target.value })}
                  />
                </label>
                <label>
                  <span>{strings.sitesBookingWhere}</span>
                  <input
                    className={styles.input}
                    value={draft.location}
                    placeholder={strings.sitesBookingWherePlaceholder}
                    onChange={(event) => edit({ location: event.target.value })}
                  />
                </label>
              </div>

              <label className={styles.bookingDescription}>
                <span>{strings.sitesBookingDescription}</span>
                <textarea
                  className={`${styles.input} ${styles.textarea}`}
                  rows={3}
                  value={draft.description}
                  onChange={(event) =>
                    edit({ description: event.target.value })
                  }
                />
              </label>

              {/* The Agenda connection, visible rather than implied: which
                  calendar the appointments are written into, and where they are
                  managed once they are there. */}
              <div className={styles.bookingCalendar}>
                <label>
                  <span>{strings.sitesBookingCalendar}</span>
                  <select
                    className={styles.input}
                    value={draft.calendarId}
                    onChange={(event) =>
                      edit({ calendarId: event.target.value })
                    }
                  >
                    {sources.map((source) => (
                      <option key={source.id} value={source.id}>
                        {source.writable
                          ? source.name
                          : strings.sitesBookingCalendarReadOnly(source.name)}
                      </option>
                    ))}
                    {boundSource === undefined && draft.calendarId !== "" && (
                      <option value={draft.calendarId}>
                        {strings.sitesBookingCalendarGone}
                      </option>
                    )}
                  </select>
                </label>
                <p className={styles.hint}>
                  {strings.sitesBookingCalendarHint}
                </p>
                {boundSource === undefined && draft.calendarId !== "" && (
                  <p className={styles.publishError} role="alert">
                    {strings.sitesBookingCalendarGoneHint}
                  </p>
                )}
                {/* Where the appointments themselves are managed: they are
                    events in a real calendar, and moving or cancelling one is
                    Agenda's job, not this screen's. */}
                <Link className={styles.liveLink} to="/agenda">
                  <CalendarDays
                    size="var(--icon-size-inline)"
                    aria-hidden="true"
                  />
                  {strings.sitesBookingOpenAgenda}
                </Link>
              </div>

              <div className={styles.bookingNumbers}>
                <label>
                  <span>{strings.sitesBookingLength}</span>
                  <input
                    className={styles.input}
                    type="number"
                    min={5}
                    value={draft.durationMinutes}
                    onChange={(event) =>
                      edit({ durationMinutes: Number(event.target.value) })
                    }
                  />
                </label>
                <label>
                  <span>{strings.sitesBookingBuffer}</span>
                  <input
                    className={styles.input}
                    type="number"
                    min={0}
                    value={draft.bufferMinutes}
                    onChange={(event) =>
                      edit({ bufferMinutes: Number(event.target.value) })
                    }
                  />
                </label>
                <label>
                  <span>{strings.sitesBookingNotice}</span>
                  <input
                    className={styles.input}
                    type="number"
                    min={0}
                    value={draft.noticeMinutes}
                    onChange={(event) =>
                      edit({ noticeMinutes: Number(event.target.value) })
                    }
                  />
                </label>
                <label>
                  <span>{strings.sitesBookingHorizon}</span>
                  <input
                    className={styles.input}
                    type="number"
                    min={1}
                    value={draft.horizonDays}
                    onChange={(event) =>
                      edit({ horizonDays: Number(event.target.value) })
                    }
                  />
                </label>
                <label>
                  <span>{strings.sitesBookingTimeZone}</span>
                  <input
                    className={`${styles.input} ${styles.mono}`}
                    value={draft.timeZone}
                    autoCapitalize="none"
                    autoCorrect="off"
                    spellCheck={false}
                    onChange={(event) => edit({ timeZone: event.target.value })}
                  />
                </label>
              </div>
              <p className={styles.hint}>{strings.sitesBookingTimeZoneHint}</p>

              {/* The week the service is offered in. Declared, never inferred:
                  an empty Sunday in the calendar is not an open Sunday. */}
              <div className={styles.catalogGroups}>
                <h3>{strings.sitesBookingHours}</h3>
                <p>{strings.sitesBookingHoursHint}</p>
                <div className={styles.catalogGroupRows}>
                  {draft.hours.map((window, index) => (
                    <div
                      className={styles.bookingHoursRow}
                      key={`${index}-${window.weekday}`}
                    >
                      <select
                        className={styles.input}
                        value={window.weekday}
                        aria-label={strings.sitesBookingDay}
                        onChange={(event) =>
                          editWindow(index, {
                            weekday: Number(event.target.value),
                          })
                        }
                      >
                        {WEEKDAYS.map((weekday) => (
                          <option key={weekday} value={weekday}>
                            {weekdayName(weekday)}
                          </option>
                        ))}
                      </select>
                      <input
                        className={styles.input}
                        type="time"
                        value={timeValue(window.startMinute)}
                        aria-label={strings.sitesBookingFrom}
                        onChange={(event) => {
                          const minutes = timeMinutes(event.target.value);
                          if (minutes !== null)
                            editWindow(index, { startMinute: minutes });
                        }}
                      />
                      <input
                        className={styles.input}
                        type="time"
                        value={timeValue(window.endMinute)}
                        aria-label={strings.sitesBookingUntil}
                        onChange={(event) => {
                          const minutes = timeMinutes(event.target.value);
                          if (minutes !== null)
                            editWindow(index, { endMinute: minutes });
                        }}
                      />
                      <Button
                        variant="ghost"
                        size="sm"
                        icon={<Trash2 size="var(--icon-size-inline)" />}
                        aria-label={strings.sitesBookingRemoveWindow(
                          windowLabel(window),
                        )}
                        onClick={() =>
                          edit({
                            hours: draft.hours.filter((_, at) => at !== index),
                          })
                        }
                      >
                        {strings.sitesCatalogGroupRemoveShort}
                      </Button>
                    </div>
                  ))}
                  <Button
                    variant="ghost"
                    size="sm"
                    icon={<Plus size="var(--icon-size-inline)" />}
                    onClick={() =>
                      edit({
                        hours: [
                          ...draft.hours,
                          blankWindow(
                            draft.hours[draft.hours.length - 1]?.weekday ?? 1,
                          ),
                        ],
                      })
                    }
                  >
                    {strings.sitesBookingAddWindow}
                  </Button>
                </div>
              </div>

              {/* The questions. Name and email are structural — they are always
                  asked and are not in this list — so the screen says that rather
                  than leaving an owner adding a name field that already exists. */}
              <div className={styles.catalogGroups}>
                <h3>{strings.sitesBookingQuestions}</h3>
                <p>{strings.sitesBookingQuestionsHint}</p>
                <div className={styles.catalogGroupRows}>
                  {draft.fields.map((field, index) => (
                    <div className={styles.bookingQuestionRow} key={index}>
                      <input
                        className={styles.input}
                        value={field.label}
                        placeholder={
                          strings.sitesBookingQuestionLabelPlaceholder
                        }
                        aria-label={strings.sitesBookingQuestionLabel}
                        onChange={(event) => {
                          const label = event.target.value;
                          // The key is suggested from the label while nobody has
                          // written one, and left alone the moment somebody has:
                          // it outlives the label and renaming a question must
                          // not orphan the answers already taken.
                          const suggested = suggestFieldKey(field.label);
                          const untouched =
                            field.key === "" || field.key === suggested;
                          editField(index, {
                            label,
                            ...(untouched
                              ? { key: suggestFieldKey(label) }
                              : {}),
                          });
                        }}
                      />
                      <input
                        className={`${styles.input} ${styles.mono}`}
                        value={field.key}
                        aria-label={strings.sitesBookingQuestionKey}
                        autoCapitalize="none"
                        autoCorrect="off"
                        spellCheck={false}
                        onChange={(event) =>
                          editField(index, { key: event.target.value })
                        }
                      />
                      <select
                        className={styles.input}
                        value={field.kind}
                        aria-label={strings.sitesBookingQuestionKind}
                        onChange={(event) =>
                          editField(index, {
                            kind: event.target.value as SiteBookingFieldKind,
                          })
                        }
                      >
                        {FIELD_KINDS.map((kind) => (
                          <option key={kind} value={kind}>
                            {fieldKindLabel(kind)}
                          </option>
                        ))}
                      </select>
                      {field.kind === "choice" && (
                        <input
                          className={styles.input}
                          value={field.options.join(", ")}
                          placeholder={
                            strings.sitesBookingQuestionOptionsPlaceholder
                          }
                          aria-label={strings.sitesBookingQuestionOptions}
                          onChange={(event) =>
                            editField(index, {
                              options: event.target.value
                                .split(",")
                                .map((option) => option.trim())
                                .filter((option) => option !== ""),
                            })
                          }
                        />
                      )}
                      <label className={styles.bookingRequired}>
                        <input
                          type="checkbox"
                          checked={field.required}
                          onChange={(event) =>
                            editField(index, { required: event.target.checked })
                          }
                        />
                        <span>{strings.sitesBookingQuestionRequired}</span>
                      </label>
                      <Button
                        variant="ghost"
                        size="sm"
                        icon={<Trash2 size="var(--icon-size-inline)" />}
                        aria-label={strings.sitesBookingRemoveQuestion(
                          field.label === "" ? field.key : field.label,
                        )}
                        onClick={() =>
                          edit({
                            fields: draft.fields.filter(
                              (_, at) => at !== index,
                            ),
                          })
                        }
                      >
                        {strings.sitesCatalogGroupRemoveShort}
                      </Button>
                    </div>
                  ))}
                  <Button
                    variant="ghost"
                    size="sm"
                    icon={<Plus size="var(--icon-size-inline)" />}
                    onClick={() =>
                      edit({ fields: [...draft.fields, blankBookingField()] })
                    }
                  >
                    {strings.sitesBookingAddQuestion}
                  </Button>
                </div>
              </div>

              <label className={styles.catalogOrdersToggle}>
                <input
                  type="checkbox"
                  checked={draft.active}
                  onChange={(event) => edit({ active: event.target.checked })}
                />
                <span>
                  <strong>{strings.sitesBookingActive}</strong>
                  <small>{strings.sitesBookingActiveHint}</small>
                </span>
              </label>

              <div className={styles.collectionActions}>
                {!creating && selected !== null && (
                  <div className={styles.collectionDisconnectGroup}>
                    <Button
                      variant={deleteArmed ? "danger" : "ghost"}
                      icon={<Trash2 size="var(--icon-size-inline)" />}
                      disabled={busy}
                      onClick={() => void remove()}
                    >
                      {deleteArmed
                        ? strings.sitesBookingDeleteConfirm
                        : strings.sitesBookingDelete}
                    </Button>
                    {deleteArmed && (
                      <span>{strings.sitesBookingDeleteHint}</span>
                    )}
                  </div>
                )}
                <Button
                  disabled={
                    busy || draft.name.trim() === "" || draft.calendarId === ""
                  }
                  onClick={() => void save()}
                >
                  {creating || selected === null
                    ? strings.sitesBookingCreate
                    : strings.sitesBookingSave}
                </Button>
              </div>
            </section>

            {/* What a visitor is offered, in the words the published page uses.
                It restates the service as it stands — never a second copy of the
                slot arithmetic, which is the server's and is computed against
                the calendar at the moment somebody asks. */}
            <section
              className={styles.catalogItemsPanel}
              aria-labelledby="booking-preview-title"
            >
              <div className={styles.collectionPanelHead}>
                <div>
                  <h2 id="booking-preview-title">
                    {strings.sitesBookingPreview}
                  </h2>
                  <p>{strings.sitesBookingPreviewHint}</p>
                </div>
              </div>
              <div className={styles.bookingPreview}>
                <strong>
                  {draft.name.trim() === ""
                    ? strings.sitesBookingUnnamed
                    : draft.name}
                </strong>
                <span>
                  {strings.sitesBookingMinutes(draft.durationMinutes)}
                </span>
                {draft.description.trim() !== "" && <p>{draft.description}</p>}
                {draft.location.trim() !== "" && (
                  <span>{strings.sitesBookingWhereLine(draft.location)}</span>
                )}
                <ul className={styles.bookingPreviewHours}>
                  {draft.hours.length === 0 ? (
                    <li>{strings.sitesBookingNoHours}</li>
                  ) : (
                    draft.hours.map((window, index) => (
                      <li key={`${index}-${window.weekday}`}>
                        {windowLabel(window)}
                      </li>
                    ))
                  )}
                </ul>
                <p className={styles.hint}>
                  {draft.fields.length === 0
                    ? strings.sitesBookingAsksNothingExtra
                    : strings.sitesBookingAsksAlso(
                        draft.fields
                          .map((field) =>
                            field.label === "" ? field.key : field.label,
                          )
                          .join(", "),
                      )}
                </p>
                <p className={styles.hint}>{strings.sitesBookingPublishHint}</p>
                {!draft.active && (
                  <p className={styles.publishError}>
                    {strings.sitesBookingOffPreview}
                  </p>
                )}
              </div>
            </section>
          </div>
        </div>
      )}
    </div>
  );
}
