// The Meet destination.
//
// Most meetings begin somewhere else — in the room they concern, or on the
// invitation that scheduled them — because that is where the people are. This
// page is for the two cases those do not cover: walking into something already
// running, and starting a call that belongs to nothing in particular.
import { useCallback, useEffect, useState } from "react";
import { Video } from "lucide-react";

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
  const [inMeeting, setInMeeting] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);

  const load = useCallback(async () => {
    try {
      setLive(await api.mine());
    } catch {
      // An empty list and a failed list look the same here on purpose: there
      // is nothing useful a person can do about the difference, and a page
      // that shouts about a hiccup teaches people to ignore it.
      setLive([]);
    }
  }, [api]);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <div className={styles.module}>
      {inMeeting !== null && (
        <MeetRoom
          meetingId={inMeeting}
          onLeft={() => {
            setInMeeting(null);
            void load();
          }}
        />
      )}

      <header className={styles.header}>
        <h1 className={styles.title}>{strings.moduleMeet}</h1>
        <Button
          variant="primary"
          disabled={starting}
          onClick={() => {
            setStarting(true);
            void api
              .start({ title: strings.meetInstantTitle })
              .then((m) => setInMeeting(m.id))
              .catch(() => undefined)
              .finally(() => {
                setStarting(false);
                void load();
              });
          }}
        >
          <Video size={16} />
          {strings.meetStartNow}
        </Button>
      </header>

      {live === null ? null : live.length === 0 ? (
        <div className={styles.empty}>
          <span className={styles.emptyMark}>
            <Video size={22} />
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
              <span className={styles.rowMark}>
                <Video size={16} />
              </span>
              <span className={styles.rowText}>
                <span className={styles.rowTitle}>
                  {m.title.trim() === "" ? strings.meetUntitled : m.title}
                </span>
                <span className={styles.rowWhen}>{since(m.startedAt)}</span>
              </span>
              <Button size="sm" onClick={() => setInMeeting(m.id)}>
                {strings.meetJoin}
              </Button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
