// A meeting on screen.
//
// The media is LiveKit's — its components SDK runs under our UI, which is
// exactly the arrangement the product doctrine describes for an integrated
// engine. What is ours is everything around it: who may be here (decided
// before a token exists), what the meeting belongs to, and what happens when
// there is no engine to talk to.
//
// The `LiveKitRoom` element wants a URL and a token. It never learns which
// workspace this is, and it cannot: the token carries an opaque room name and
// a user id, and this component has nothing else to give it.
import { useEffect, useState } from "react";
import "@livekit/components-styles";
import {
  LiveKitRoom,
  PreJoin,
  VideoConference,
  formatChatMessageLinks,
} from "@livekit/components-react";
import type { LocalUserChoices } from "@livekit/components-react";
import { PhoneOff, Video } from "lucide-react";

import { Button } from "../ds";
import { strings } from "../i18n";
import { MeetUnavailable, useMeetApi } from "./api";
import type { JoinGrant } from "./api";
import styles from "./MeetRoom.module.css";

/**
 * Join a meeting and hold it until the person leaves.
 *
 * `onLeft` fires when they hang up or the connection ends, so whatever opened
 * this can put the screen back as it was.
 */
export function MeetRoom({
  meetingId,
  onLeft,
}: {
  meetingId: string;
  onLeft: () => void;
}) {
  const api = useMeetApi();
  const [grant, setGrant] = useState<JoinGrant | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  // Which camera and microphone, checked before joining rather than
  // discovered after. Somebody who joins broken usually leaves rather than
  // hunting for a settings menu mid-meeting.
  const [choices, setChoices] = useState<LocalUserChoices | null>(null);

  useEffect(() => {
    let joined = true;
    void (async () => {
      try {
        const g = await api.join(meetingId);
        // The person may have left before the token arrived; joining a room
        // nobody is looking at would hold a camera open.
        if (joined) setGrant(g);
      } catch (failure) {
        if (!joined) return;
        setProblem(
          failure instanceof MeetUnavailable
            ? strings.meetNoEngine
            : strings.meetJoinFailed,
        );
      }
    })();
    return () => {
      joined = false;
    };
  }, [api, meetingId]);

  if (problem !== null) {
    return (
      <div className={styles.notice}>
        <Video size={20} className={styles.noticeMark} />
        <p className={styles.noticeText}>{problem}</p>
        <Button variant="ghost" onClick={onLeft}>
          {strings.meetClose}
        </Button>
      </div>
    );
  }

  if (grant === null) {
    return <div className={styles.notice}>{strings.meetJoining}</div>;
  }

  if (choices === null) {
    return (
      <div className={styles.room} data-lk-theme="default">
        <div className={styles.prejoin}>
          <PreJoin
            defaults={{
              // The same default the call itself uses: heard, and seen only
              // by choice.
              audioEnabled: true,
              videoEnabled: false,
            }}
            onSubmit={setChoices}
            onError={() => setProblem(strings.meetJoinFailed)}
            joinLabel={strings.meetJoinNow}
            micLabel={strings.meetMicrophone}
            camLabel={strings.meetCamera}
            persistUserChoices
          />
        </div>
      </div>
    );
  }

  return (
    <div className={styles.room}>
      <LiveKitRoom
        serverUrl={grant.url}
        token={grant.token}
        // Connect straight away: the person pressed join, and asking again
        // inside the room would be asking twice.
        connect
        // Camera off, microphone on. Somebody joining a call expects to be
        // heard and to choose to be seen — the reverse surprises people in a
        // way that is hard to undo.
        video={choices.videoEnabled}
        audio={choices.audioEnabled}
        onDisconnected={onLeft}
        className={styles.livekit}
        // The engine's own theme attribute. Without it none of its CSS
        // variables exist, and `.lk-grid-layout-wrapper`'s
        // `height: calc(100% - var(--lk-control-bar-height))` becomes invalid
        // — so the video area collapsed to zero and the call rendered as an
        // empty black screen. A day of CSS guesses; one missing attribute.
        data-lk-theme="default"
      >
        <VideoConference chatMessageFormatter={formatChatMessageLinks} />
      </LiveKitRoom>
      <button
        type="button"
        className={styles.leave}
        onClick={onLeft}
        aria-label={strings.meetLeave}
        title={strings.meetLeave}
      >
        <PhoneOff size={16} />
        {strings.meetLeave}
      </button>
    </div>
  );
}
