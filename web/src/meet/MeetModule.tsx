import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  AlertCircle,
  ArrowRight,
  CalendarDays,
  Clock3,
  Keyboard,
  RefreshCw,
  ShieldCheck,
  Video,
} from "lucide-react";

import { useAuth } from "../auth";
import { Button } from "../ds";
import { getLocale, strings } from "../i18n";
import { useJmapClient, type CalendarEvent } from "../jmap";
import { MeetRoom } from "./MeetRoom";
import { useMeetApi, type Meeting } from "./api";
import styles from "./MeetModule.module.css";

function greeting(): string {
  const hour = new Date().getHours();
  if (hour < 12) return strings.homeGreetingMorning;
  if (hour < 18) return strings.homeGreetingAfternoon;
  return strings.homeGreetingEvening;
}

function meetingId(value: string): string | null {
  const text = value.trim();
  if (text === "") return null;
  try {
    const url = new URL(text, window.location.origin);
    const fromQuery = url.searchParams.get("meeting");
    if (fromQuery !== null && fromQuery.trim() !== "") return fromQuery.trim();
  } catch {
    /* A bare meeting id is valid input. */
  }
  return /^[A-Za-z0-9_-]{8,128}$/.test(text) ? text : null;
}

function time(iso: string): string {
  return new Date(iso).toLocaleTimeString(getLocale(), {
    hour: "2-digit",
    minute: "2-digit",
  });
}

function duration(event: CalendarEvent): string {
  const minutes = Math.max(
    1,
    Math.round((new Date(event.endsAt).getTime() - new Date(event.startsAt).getTime()) / 60_000),
  );
  return minutes < 60 ? `${minutes} min` : `${Math.floor(minutes / 60)}h${minutes % 60 ? ` ${minutes % 60}m` : ""}`;
}

export function MeetModule() {
  const api = useMeetApi();
  const calendar = useJmapClient();
  const { identity } = useAuth();
  const navigate = useNavigate();
  const [live, setLive] = useState<Meeting[] | null>(null);
  const [events, setEvents] = useState<CalendarEvent[]>([]);
  const [inMeeting, setInMeeting] = useState<string | null>(() =>
    new URLSearchParams(window.location.search).get("meeting"),
  );
  const [joinText, setJoinText] = useState("");
  const [joinProblem, setJoinProblem] = useState(false);
  const [starting, setStarting] = useState(false);
  const [loadProblem, setLoadProblem] = useState(false);
  const [startProblem, setStartProblem] = useState(false);

  const firstName = useMemo(() => {
    const name = identity?.name?.trim();
    if (name !== undefined && name !== "") return name.split(/\s+/)[0] ?? name;
    return identity?.email?.split("@")[0] ?? "";
  }, [identity]);

  const load = useCallback(async () => {
    const now = new Date();
    const end = new Date(now.getTime() + 14 * 86_400_000);
    const [meetings, upcoming] = await Promise.allSettled([
      api.mine(),
      calendar.calendarEvents(now.toISOString(), end.toISOString()),
    ]);
    if (meetings.status === "fulfilled") {
      setLive(meetings.value);
      setLoadProblem(false);
    } else {
      setLive(null);
      setLoadProblem(true);
    }
    setEvents(
      upcoming.status === "fulfilled"
        ? upcoming.value.sort((a, b) => a.startsAt.localeCompare(b.startsAt))
        : [],
    );
  }, [api, calendar]);

  useEffect(() => { void load(); }, [load]);

  const enter = useCallback((id: string) => {
    window.history.replaceState(null, "", `/meet?meeting=${encodeURIComponent(id)}`);
    setInMeeting(id);
  }, []);

  const leaveMeeting = useCallback(() => {
    window.history.replaceState(null, "", "/meet");
    setInMeeting(null);
    void load();
  }, [load]);

  const startMeeting = () => {
    setStarting(true);
    setStartProblem(false);
    void api.start({ title: strings.meetInstantTitle })
      .then((meeting) => enter(meeting.id))
      .catch(() => setStartProblem(true))
      .finally(() => { setStarting(false); void load(); });
  };

  const join = (event: React.FormEvent) => {
    event.preventDefault();
    const id = meetingId(joinText);
    if (id === null) { setJoinProblem(true); return; }
    setJoinProblem(false);
    enter(id);
  };

  const now = new Date();
  const today = events.filter((event) => new Date(event.startsAt).toDateString() === now.toDateString());
  const later = events.filter((event) => new Date(event.startsAt).toDateString() !== now.toDateString());
  const month = new Intl.DateTimeFormat(getLocale(), { month: "long", year: "numeric" }).format(now);

  return (
    <div className={styles.module}>
      {inMeeting !== null && <MeetRoom meetingId={inMeeting} onLeft={leaveMeeting} />}
      <div className={styles.content}>
        <div className={styles.topbar}>
          <form className={styles.joinForm} onSubmit={join}>
            <Keyboard aria-hidden="true" />
            <input value={joinText} onChange={(e) => { setJoinText(e.target.value); setJoinProblem(false); }} placeholder={strings.meetJoinPlaceholder} aria-label={strings.meetJoinPlaceholder} />
            <button type="submit" disabled={joinText.trim() === ""}>{strings.meetJoinShort}</button>
          </form>
          <Button icon={<Video aria-hidden="true" />} onClick={startMeeting} disabled={starting}>{strings.meetNew}</Button>
        </div>

        <header className={styles.header}>
          <p className={styles.greeting}>{greeting()}{firstName ? `, ${firstName}` : ""} <span aria-hidden="true">👋</span></p>
          <h1>{strings.meetYourSpaceLead} <em>{strings.meetYourSpaceAccent}</em></h1>
          <p>{strings.meetSubtitle}</p>
        </header>

        {(joinProblem || startProblem) && <div className={styles.inlineError} role="alert"><AlertCircle aria-hidden="true" />{joinProblem ? strings.meetJoinInputInvalid : strings.meetStartFailed}</div>}

        <div className={styles.dashboard}>
          <main className={styles.primary}>
            <section className={styles.hero}>
              <div className={styles.heroCopy}>
                <span className={styles.heroMark}><Video aria-hidden="true" /></span>
                <h2>{strings.meetHeroNewTitle}</h2>
                <p>{strings.meetHeroNewText}</p>
                <div className={styles.heroActions}>
                  <Button icon={<Video aria-hidden="true" />} onClick={startMeeting} disabled={starting}>{starting ? strings.meetStarting : strings.meetStartNow}</Button>
                  <Button variant="ghost" icon={<CalendarDays aria-hidden="true" />} onClick={() => navigate("/agenda")}>{strings.meetSchedule}</Button>
                </div>
              </div>
              <div className={styles.heroScene} aria-hidden="true"><span className={styles.hand}>✋</span><span className={styles.mug} /><span className={styles.laptop}><Video /></span></div>
            </section>

            <section className={styles.meetings}>
              <div className={styles.sectionHeading}><div><h2>{strings.meetHappeningNow}</h2><p>{strings.meetHappeningHint}</p></div>{live !== null && <span className={styles.count}>{strings.meetLiveCount(live.length)}</span>}</div>
              {loadProblem ? <div className={styles.state} role="alert"><AlertCircle /><strong>{strings.meetLoadFailed}</strong><Button variant="ghost" icon={<RefreshCw />} onClick={() => void load()}>{strings.meetRetry}</Button></div>
                : live === null ? <div className={styles.loading}><span /><span /></div>
                : live.length === 0 ? <div className={styles.empty}><Video /><strong>{strings.meetNothingLive}</strong><p>{strings.meetWhereFrom}</p></div>
                : <ul className={styles.list}>{live.map((meeting) => <li key={meeting.id} className={styles.card}><div className={styles.cardTop}><span className={styles.cardIcon}><Video /></span><span className={styles.livePill}><i />{strings.meetLive}</span></div><h3>{meeting.title.trim() || strings.meetUntitled}</h3><p>{meeting.startedAt === null ? strings.meetNotStarted : strings.meetStartedAt(time(meeting.startedAt))}</p><Button onClick={() => enter(meeting.id)}>{strings.meetJoin}<ArrowRight /></Button></li>)}</ul>}
            </section>

            {later.length > 0 && <section className={styles.upcoming}><div className={styles.sectionHeading}><div><h2>{strings.meetUpcoming}</h2><p>{strings.meetUpcomingHint}</p></div></div>{later.slice(0, 4).map((event) => <button type="button" key={`${event.id}-${event.startsAt}`} onClick={() => navigate("/agenda")}><time><b>{new Date(event.startsAt).toLocaleDateString(getLocale(), { day: "2-digit" })}</b><small>{new Date(event.startsAt).toLocaleDateString(getLocale(), { month: "short" })}</small></time><span><strong>{event.summary || strings.meetCalendarUntitled}</strong><small>{time(event.startsAt)} · {duration(event)}</small></span><ArrowRight /></button>)}</section>}
          </main>

          <aside className={styles.sidebar}>
            <section className={styles.dateCard}><div><span>{month}</span><CalendarDays /></div><strong>{now.getDate()}</strong><p>{new Intl.DateTimeFormat(getLocale(), { weekday: "long" }).format(now)}</p></section>
            <section className={styles.safety}><ShieldCheck /><div><strong>{strings.meetSafetyTitle}</strong><p>{strings.meetSafetyBody}</p></div></section>
            <section className={styles.schedule}><div className={styles.asideHeading}><h2>{strings.meetTodaySchedule}</h2><button onClick={() => navigate("/agenda")}><CalendarDays />{strings.meetOpenAgenda}</button></div>{today.length === 0 ? <div className={styles.noEvents}><Clock3 /><p>{strings.meetNoEventsToday}</p></div> : <ul>{today.slice(0, 6).map((event) => <li key={`${event.id}-${event.startsAt}`}><time>{time(event.startsAt)}</time><span><strong>{event.summary || strings.meetCalendarUntitled}</strong><small>{duration(event)}</small></span></li>)}</ul>}<button className={styles.viewAgenda} onClick={() => navigate("/agenda")}>{strings.meetViewAgenda}<ArrowRight /></button></section>
            <section className={styles.quick}><h2>{strings.meetQuickActions}</h2><div><button onClick={startMeeting}><Video />{strings.meetStartNow}</button><button onClick={() => navigate("/agenda")}><CalendarDays />{strings.meetSchedule}</button><button onClick={() => document.querySelector<HTMLInputElement>(`.${styles.joinForm} input`)?.focus()}><Keyboard />{strings.meetJoinShort}</button></div></section>
          </aside>
        </div>
      </div>
    </div>
  );
}
