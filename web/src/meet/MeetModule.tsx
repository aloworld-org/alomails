// The Meet destination.
//
// Most meetings begin somewhere else — in the room they concern, or on the
// invitation that scheduled them — because that is where the people are. This
// page is for the two cases those do not cover: walking into something already
// running, and starting a call that belongs to nothing in particular.
import { useCallback, useEffect, useState } from "react";
import { AlertCircle, ArrowRight, RefreshCw, Video } from "lucide-react";

import { Button } from "../ds";
import { strings } from "../i18n";
import { MeetRoom } from "./MeetRoom";
import { useMeetApi } from "./api";
import type { Meeting } from "./api";
import styles from "./MeetModule.module.css";

/** Local time of day, for a meeting that started earlier. */
function since(iso: string | null): string {
  if (iso === null) return strings.meetNotStarted;
  return new Date(iso).toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function MeetModule() {
  const api = useMeetApi();
  const [live, setLive] = useState<Meeting[] | null>(null);
  const [inMeeting, setInMeeting] = useState<string | null>(() => new URLSearchParams(window.location.search).get("meeting"));
  const [starting, setStarting] = useState(false);
  const [loadProblem, setLoadProblem] = useState(false);
  const [startProblem, setStartProblem] = useState(false);

  const load = useCallback(async () => {
    try {
      setLive(await api.mine());
      setLoadProblem(false);
    } catch {
      setLive(null);
      setLoadProblem(true);
    }
  }, [api]);

  useEffect(() => {
    void load();
  }, [load]);

  const startMeeting = () => {
    setStarting(true);
    setStartProblem(false);
    void api
      .start({ title: strings.meetInstantTitle })
      .then((meeting) => {
        window.history.replaceState(null, "", `/meet?meeting=${encodeURIComponent(meeting.id)}`);
        setInMeeting(meeting.id);
      })
      .catch(() => setStartProblem(true))
      .finally(() => {
        setStarting(false);
        void load();
      });
  };
  const liveCount =
    live === null
      ? ""
      : typeof strings.meetLiveCount === "function"
        ? strings.meetLiveCount(live.length)
        : `${live.length} ${strings.meetTitle}`;

  return (
    <div className={styles.module}>
      {inMeeting !== null && (
        <MeetRoom
          meetingId={inMeeting}
          onLeft={() => {
            window.history.replaceState(null, "", "/meet");
            setInMeeting(null);
            void load();
          }}
        />
      )}

      <div className={styles.content}>
        <header className={styles.header}>
          <span className={styles.eyebrow}>{strings.meetEyebrow}</span>
          <h1 className={styles.title}>{strings.moduleMeet}</h1>
          <p className={styles.subtitle}>{strings.meetSubtitle}</p>
        </header>

        <section className={styles.hero} aria-labelledby="meet-hero-title">
          <div className={styles.heroCopy}>
            <span className={styles.heroMark}><Video aria-hidden="true" /></span>
            <h2 id="meet-hero-title" className={styles.heroTitle}>{strings.meetHeroTitle}</h2>
            <p className={styles.heroText}>{strings.meetHeroText}</p>
            <Button
              className={styles.startButton}
              disabled={starting}
              onClick={startMeeting}
              icon={<Video aria-hidden="true" />}
            >
              {starting ? strings.meetStarting : strings.meetStartNow}
            </Button>
          </div>
          <div className={styles.heroVisual} aria-hidden="true">
            <span className={styles.person}><span /></span>
            <span className={styles.signal}><i /><i /><i /></span>
          </div>
        </section>

        {startProblem && (
          <div className={styles.inlineError} role="alert">
            <AlertCircle aria-hidden="true" />
            <span>{strings.meetStartFailed}</span>
          </div>
        )}

        <div className={styles.sectionHeading}>
          <div>
            <h2>{strings.meetHappeningNow}</h2>
            <p>{strings.meetHappeningHint}</p>
          </div>
          {live !== null && !loadProblem && (
            <span className={styles.count}>{liveCount}</span>
          )}
        </div>

        {loadProblem ? (
        <div className={styles.state} role="alert">
          <span className={styles.stateMark}>
            <AlertCircle aria-hidden="true" />
          </span>
          <p className={styles.stateTitle}>{strings.meetLoadFailed}</p>
          <p className={styles.stateHint}>{strings.meetLoadFailedHint}</p>
          <Button variant="ghost" icon={<RefreshCw aria-hidden="true" />} onClick={() => void load()}>
            {strings.meetRetry}
          </Button>
        </div>
      ) : live === null ? (
        <div className={styles.loading} aria-label={strings.meetLoading} aria-busy="true">
          <span />
          <span />
          <span />
        </div>
        ) : live.length === 0 ? (
        <div className={styles.empty}>
          <span className={styles.emptyMark}>
            <Video aria-hidden="true" />
          </span>
          <p className={styles.emptyTitle}>{strings.meetNothingLive}</p>
          {/* Says where meetings actually come from, because a page offering
              only one button teaches that one button is all there is. */}
          <p className={styles.emptyHint}>{strings.meetWhereFrom}</p>
        </div>
        ) : (
        <ul className={styles.list}>
          {live.map((m) => (
            <li key={m.id} className={styles.row}>
              <div className={styles.cardTop}>
                <span className={styles.rowMark}><Video aria-hidden="true" /></span>
                <span className={m.startedAt === null ? styles.readyPill : styles.livePill}>
                  <i />
                  {m.startedAt === null ? strings.meetReady : strings.meetLive}
                </span>
              </div>
              <span className={styles.rowText}>
                <span className={styles.rowTitle}>
                  {m.title.trim() === "" ? strings.meetUntitled : m.title}
                </span>
                <span className={styles.rowWhen}>
                  {m.startedAt === null ? strings.meetNotStarted : strings.meetStartedAt(since(m.startedAt))}
                </span>
              </span>
              <Button className={styles.joinButton} onClick={() => {
                window.history.replaceState(null, "", `/meet?meeting=${encodeURIComponent(m.id)}`);
                setInMeeting(m.id);
              }}>
                {strings.meetJoin}
                <ArrowRight aria-hidden="true" />
              </Button>
            </li>
          ))}
        </ul>
        )}
      </div>
    </div>
  );
}
