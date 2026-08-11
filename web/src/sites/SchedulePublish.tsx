// Publishing a website at a chosen moment (S2.05b): the control that says
// "go live on Monday at 09:00" instead of "go live now", what it is going to
// do, and calling it off.
//
// Two decisions this screen is built on.
//
// **Time is the reader's, always.** The server speaks UTC instants; a person
// speaks "Monday at nine". The picker is a plain `datetime-local`, which is
// the browser's own local-time control, and every moment shown is formatted in
// the reader's zone — with the zone NAMED next to the picker, because a person
// scheduling a launch from a hotel in another country must be able to see
// which nine o'clock they picked.
//
// **A schedule is not a publish.** Nothing here changes what the internet is
// serving. Scheduling, moving and cancelling are all reversible with one
// click, so none of them asks for confirmation (ux-principles law 7); the only
// irreversible thing is the publish itself, and that is the moment's own doing.
import { useCallback, useEffect, useRef, useState } from "react";
import { CalendarClock, X } from "lucide-react";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import type { SitePublishSchedule } from "./types";
import styles from "./SitesModule.module.css";

/** How a moment is named on this screen: a weekday a person recognises, plus
 *  the time they chose. Never the raw instant. */
const moment = new Intl.DateTimeFormat(undefined, {
  dateStyle: "full",
  timeStyle: "short",
});

/** The reader's own time zone, as the browser resolves it (`Europe/Amsterdam`).
 *  Falls back to the empty string on the rare browser that has no zone to give,
 *  which simply drops the explanation rather than showing "undefined". */
function localZone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone ?? "";
  } catch {
    return "";
  }
}

/** The value a `datetime-local` input wants (`YYYY-MM-DDTHH:mm`), in local
 *  time — which is exactly what that control means by its value. */
function toInputValue(date: Date): string {
  const pad = (value: number) => String(value).padStart(2, "0");
  return (
    `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}` +
    `T${pad(date.getHours())}:${pad(date.getMinutes())}`
  );
}

/** What the picker offers when nothing is scheduled yet: tomorrow morning —
 *  a real proposal rather than an empty field the person must decode. */
function defaultMoment(): string {
  const suggestion = new Date();
  suggestion.setDate(suggestion.getDate() + 1);
  suggestion.setHours(9, 0, 0, 0);
  return toInputValue(suggestion);
}

/** The most recent finished intention, if the last thing that happened is
 *  worth telling the owner about. A published or refused schedule is; a
 *  cancelled one is only interesting until the page reloads. */
function lastOutcome(history: SitePublishSchedule[]): SitePublishSchedule | null {
  const finished = history.find(
    (entry) => entry.status === "published" || entry.status === "failed",
  );
  return finished ?? null;
}

export function SchedulePublish({
  siteId,
  onPublished,
}: {
  siteId: string;
  /** Called when a scheduled publish turns out to have happened, so the site
   *  header can stop saying "draft" without the person reloading. */
  onPublished?: () => void;
}) {
  const api = useSitesApi();
  const [schedule, setSchedule] = useState<SitePublishSchedule | null>(null);
  const [history, setHistory] = useState<SitePublishSchedule[]>([]);
  const [picking, setPicking] = useState(false);
  const [chosen, setChosen] = useState(defaultMoment);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [cancelled, setCancelled] = useState<SitePublishSchedule | null>(null);

  const load = useCallback(async () => {
    try {
      const answer = await api.publishSchedule(siteId);
      setSchedule(answer.schedule);
      setHistory(answer.history);
      setError(null);
      return answer;
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesScheduleLoadFailed));
      return null;
    } finally {
      setLoading(false);
    }
  }, [api, siteId]);

  useEffect(() => {
    void load();
  }, [load]);

  // The parent's callback is held in a ref rather than watched as a
  // dependency: an inline arrow from the site screen changes identity on every
  // one of its renders, and a poll that restarts on every render never fires.
  const notify = useRef(onPublished);
  useEffect(() => {
    notify.current = onPublished;
  }, [onPublished]);

  // A schedule that is waiting will fire while this screen is open, and the
  // person who scheduled it is exactly the person watching for it. One poll a
  // minute is enough to turn "publishes on…" into "published on…" by itself.
  const waitingFor = schedule === null ? null : schedule.id;
  useEffect(() => {
    if (waitingFor === null) return;
    const timer = setInterval(() => {
      void load().then((answer) => {
        if (answer !== null && answer.schedule === null) notify.current?.();
      });
    }, 60_000);
    return () => clearInterval(timer);
  }, [load, waitingFor]);

  function open() {
    setChosen(
      schedule === null
        ? defaultMoment()
        : toInputValue(new Date(schedule.publishAt)),
    );
    setCancelled(null);
    setError(null);
    setPicking(true);
  }

  async function save() {
    // `datetime-local` gives local wall-clock time; `Date` reads it in the
    // reader's own zone and `toISOString` turns it into the instant the
    // server stores. The two never disagree about which nine o'clock it was.
    const at = new Date(chosen);
    if (Number.isNaN(at.getTime())) {
      setError(strings.sitesScheduleMissingMoment);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      setSchedule(await api.scheduleSitePublish(siteId, at.toISOString()));
      setPicking(false);
      setCancelled(null);
      await load();
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesScheduleSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  async function callOff(entry: SitePublishSchedule) {
    setBusy(true);
    setError(null);
    try {
      setCancelled(await api.cancelSitePublish(siteId, entry.id));
      setPicking(false);
      await load();
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesScheduleCancelFailed));
    } finally {
      setBusy(false);
    }
  }

  const zone = localZone();
  const outcome = schedule === null ? lastOutcome(history) : null;
  const chosenMoment = new Date(chosen);
  const chosenReadable = Number.isNaN(chosenMoment.getTime())
    ? null
    : moment.format(chosenMoment);

  return (
    <section
      className={styles.schedulePanel}
      aria-labelledby="site-schedule-title"
    >
      <div className={styles.scheduleSummary}>
        <span className={styles.scheduleIcon} aria-hidden="true">
          <CalendarClock />
        </span>
        <div className={styles.scheduleCopy}>
          <h2 id="site-schedule-title" className={styles.scheduleTitle}>
            {strings.sitesScheduleTitle}
          </h2>
          {loading ? (
            <p className={styles.scheduleHint}>
              <Spinner size={14} /> {strings.sitesScheduleLoading}
            </p>
          ) : schedule !== null ? (
            <p className={styles.scheduleHint} role="status">
              {schedule.status === "publishing"
                ? strings.sitesSchedulePublishingNow
                : strings.sitesSchedulePending(
                    moment.format(new Date(schedule.publishAt)),
                  )}
            </p>
          ) : cancelled !== null ? (
            <p className={styles.scheduleHint} role="status">
              {strings.sitesScheduleCancelled(
                moment.format(new Date(cancelled.publishAt)),
              )}
            </p>
          ) : outcome !== null ? (
            <p className={styles.scheduleHint} role="status">
              {outcome.status === "published"
                ? strings.sitesScheduleDone(
                    moment.format(new Date(outcome.publishAt)),
                  )
                : strings.sitesScheduleFailed(
                    moment.format(new Date(outcome.publishAt)),
                    outcome.lastError ?? "",
                  )}
            </p>
          ) : (
            <p className={styles.scheduleHint}>{strings.sitesScheduleHint}</p>
          )}
        </div>
        <div className={styles.scheduleActions}>
          {schedule !== null && schedule.status === "scheduled" && (
            <Button
              variant="ghost"
              size="sm"
              icon={<X size="var(--icon-size-inline)" />}
              disabled={busy}
              onClick={() => void callOff(schedule)}
            >
              {busy ? strings.sitesScheduleCancelling : strings.sitesScheduleCancel}
            </Button>
          )}
          {!picking && schedule?.status !== "publishing" && (
            <Button
              variant="ghost"
              size="sm"
              icon={<CalendarClock size="var(--icon-size-inline)" />}
              disabled={busy}
              onClick={open}
            >
              {schedule === null
                ? strings.sitesScheduleOpen
                : strings.sitesScheduleChange}
            </Button>
          )}
        </div>
      </div>

      {picking && (
        <div className={styles.scheduleForm}>
          <label className={styles.scheduleField}>
            <span>{strings.sitesScheduleWhen}</span>
            <input
              className={styles.input}
              type="datetime-local"
              value={chosen}
              disabled={busy}
              onChange={(event) => setChosen(event.target.value)}
            />
          </label>
          <p className={styles.scheduleZone}>
            {chosenReadable !== null && (
              <strong>{strings.sitesScheduleGoesLive(chosenReadable)}</strong>
            )}
            {zone !== "" && <span>{strings.sitesScheduleTimeZone(zone)}</span>}
          </p>
          <div className={styles.scheduleFormActions}>
            <Button
              variant="ghost"
              size="sm"
              disabled={busy}
              onClick={() => {
                setPicking(false);
                setError(null);
              }}
            >
              {strings.cancel}
            </Button>
            <Button size="sm" disabled={busy} onClick={() => void save()}>
              {busy
                ? strings.sitesScheduleSaving
                : schedule === null
                  ? strings.sitesScheduleSave
                  : strings.sitesScheduleMove}
            </Button>
          </div>
        </div>
      )}

      {error !== null && (
        <p className={styles.publishError} role="alert">
          {error}
        </p>
      )}
    </section>
  );
}
