// The meetings that have already happened — and the record of one of them.
//
// A meeting is a record: it ended, and what was said in it is kept (ADR 0034's
// Meet agent reads it with `meeting_record`). Until now this list was six
// lines of text with nothing to open, so the record had no surface; the focus
// button beside each line puts one meeting in focus and shows its agent under
// the list (A8.4) — where the meeting came from, what @meet can do with it,
// and a question about it answered in place.
//
// One meeting at a time, and clicking the focused one again lets it go: the
// panel below the list belongs to the line the person pointed at, and a second
// panel would only raise the question of which meeting it is about.
import { useState } from "react";
import { Bot, Video } from "lucide-react";

import { RecordAgentPanel } from "../agents";
import type { RecordOrigin } from "../agents";
import { Card } from "../ds";
import { getLocale, strings } from "../i18n";
import type { Meeting } from "./api";
import styles from "./MeetModule.module.css";

/** How many of the recent meetings the panel lists. */
const SHOWN = 6;

/** The words on the line, and the words a question about it uses. */
export function meetingTitle(meeting: Meeting): string {
  return meeting.title.trim() || strings.meetUntitled;
}

/** Where a meeting came from, as the meeting itself carries it: the
 *  conversation it was started from, or the calendar entry it was scheduled
 *  as. `createdBy` is an account id and is deliberately not used — an opaque
 *  subject is not an origin anybody can read (A8.4, AW.2). */
export function meetingOrigin(meeting: Meeting): RecordOrigin | null {
  if (meeting.channel !== null) {
    return { kind: "thread", id: meeting.channel, label: null };
  }
  if (meeting.event !== null) {
    return { kind: "event", id: meeting.event, label: null };
  }
  return null;
}

/** How long a finished meeting ran, in whole minutes, or `null` while a
 *  meeting has no two ends to measure between. */
function ranFor(meeting: Meeting): number | null {
  if (meeting.startedAt === null || meeting.endedAt === null) return null;
  const minutes = Math.round(
    (new Date(meeting.endedAt).getTime() -
      new Date(meeting.startedAt).getTime()) /
      60_000,
  );
  return Math.max(1, minutes);
}

export function RecentMeetings({ history }: { history: Meeting[] }) {
  const [focused, setFocused] = useState<string | null>(null);
  const shown = history.slice(0, SHOWN);
  const inFocus = shown.find((meeting) => meeting.id === focused) ?? null;

  return (
    <Card as="section" pad="sm" className={styles.history}>
      <div className={styles.sectionHeading}>
        <div>
          <h2>{strings.meetRecent}</h2>
          <p>{strings.meetRecentHint}</p>
        </div>
      </div>
      <ul>
        {shown.map((meeting) => {
          const minutes = ranFor(meeting);
          const on = meeting.id === focused;
          return (
            <li key={meeting.id}>
              <span className={styles.historyIcon}>
                <Video />
              </span>
              <div>
                <strong>{meetingTitle(meeting)}</strong>
                <small>
                  {meeting.endedAt === null
                    ? ""
                    : strings.meetEndedAt(
                        new Date(meeting.endedAt).toLocaleString(getLocale(), {
                          dateStyle: "medium",
                          timeStyle: "short",
                        }),
                      )}
                </small>
              </div>
              <span className="flex items-center gap-2 justify-self-end">
                {minutes !== null && <time>{strings.meetDuration(minutes)}</time>}
                <button
                  type="button"
                  className={`flex size-9 shrink-0 items-center justify-center rounded-lg border-0 ${
                    on
                      ? "bg-accent-soft text-accent"
                      : "bg-transparent text-tertiary hover:bg-raised hover:text-primary"
                  }`}
                  aria-pressed={on}
                  aria-label={strings.recordAgentFocusRecord(
                    meetingTitle(meeting),
                  )}
                  title={strings.recordAgentPanelToggle}
                  onClick={() => setFocused(on ? null : meeting.id)}
                >
                  <Bot size={15} />
                </button>
              </span>
            </li>
          );
        })}
      </ul>
      {inFocus !== null && (
        <div className="mt-3">
          <RecordAgentPanel
            product="meet"
            recordKind="meeting"
            recordId={inFocus.id}
            recordLabel={meetingTitle(inFocus)}
            origin={meetingOrigin(inFocus)}
          />
        </div>
      )}
    </Card>
  );
}
