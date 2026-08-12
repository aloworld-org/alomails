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
  useDataChannel,
  useLocalParticipant,
} from "@livekit/components-react";
import type { LocalUserChoices } from "@livekit/components-react";
import { ArrowLeft, Copy, Hand, MonitorUp, PhoneOff, RefreshCw, ServerOff, Share2, Smile, Video } from "lucide-react";

import { Button } from "../ds";
import { strings } from "../i18n";
import { MeetUnavailable, useMeetApi } from "./api";
import type { JoinGrant } from "./api";
import styles from "./MeetRoom.module.css";

function useMeetingDuration(startedAt: string | null): string {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1_000);
    return () => window.clearInterval(timer);
  }, []);
  if (startedAt === null) return "00:00";
  const seconds = Math.max(0, Math.floor((now - new Date(startedAt).getTime()) / 1_000));
  const hours = Math.floor(seconds / 3_600);
  const minutes = Math.floor((seconds % 3_600) / 60);
  const rest = seconds % 60;
  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, "0")}:${String(rest).padStart(2, "0")}`
    : `${String(minutes).padStart(2, "0")}:${String(rest).padStart(2, "0")}`;
}

function MeetingHeader({ grant }: { grant: JoinGrant }) {
  const duration = useMeetingDuration(grant.meeting.startedAt);
  const title = grant.meeting.title.trim() === "" ? strings.meetUntitled : grant.meeting.title;
  return (
    <div className={styles.roomHeader}>
      <span className={styles.roomBrand}><Video aria-hidden="true" /></span>
      <div className={styles.roomIdentity}>
        <strong>{title}</strong>
        <span>
          <i />
          {strings.meetLive}
          <b aria-hidden="true">·</b>
          <time>{duration}</time>
        </span>
      </div>
    </div>
  );
}

function PresentingNotice() {
  const { isScreenShareEnabled } = useLocalParticipant();
  if (!isScreenShareEnabled) return null;
  return (
    <div className={styles.presenting} role="status">
      <span><MonitorUp aria-hidden="true" /></span>
      <strong>{strings.meetPresentingTitle}</strong>
      <p>{strings.meetPresentingBody}</p>
    </div>
  );
}

type MeetSignal =
  | { kind: "hand"; raised: boolean; name: string }
  | { kind: "reaction"; emoji: string; name: string };

function MeetingActions({ meetingId }: { meetingId: string }) {
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();
  const { localParticipant } = useLocalParticipant();
  const [handRaised, setHandRaised] = useState(false);
  const [raisedHands, setRaisedHands] = useState<string[]>([]);
  const [reaction, setReaction] = useState<{ emoji: string; name: string; key: number } | null>(null);
  const [copied, setCopied] = useState(false);
  const { send } = useDataChannel("alo-meet-actions", (message) => {
    try {
      const signal = JSON.parse(decoder.decode(message.payload)) as MeetSignal;
      if (signal.kind === "hand") {
        setRaisedHands((current) => signal.raised
          ? Array.from(new Set([...current, signal.name]))
          : current.filter((name) => name !== signal.name));
      } else if (signal.kind === "reaction") {
        setReaction({ emoji: signal.emoji, name: signal.name, key: Date.now() });
      }
    } catch {
      // Data-channel topics are shared with older clients. Unknown payloads
      // are ignored rather than breaking a call already in progress.
    }
  });
  const name = localParticipant.name || strings.meetSomeone;

  const broadcast = async (signal: MeetSignal) => {
    await send(encoder.encode(JSON.stringify(signal)), { reliable: true });
  };
  const toggleHand = () => {
    const raised = !handRaised;
    setHandRaised(raised);
    setRaisedHands((current) => raised
      ? Array.from(new Set([...current, name]))
      : current.filter((person) => person !== name));
    void broadcast({ kind: "hand", raised, name });
  };
  const react = (emoji: string) => {
    setReaction({ emoji, name, key: Date.now() });
    void broadcast({ kind: "reaction", emoji, name });
  };
  const share = async () => {
    const url = `${window.location.origin}/meet?meeting=${encodeURIComponent(meetingId)}`;
    if (navigator.share !== undefined) {
      await navigator.share({ title: strings.meetInviteTitle, text: strings.meetInviteText, url });
    } else {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2_000);
    }
  };

  return (
    <>
      <div className={styles.meetingActions}>
        <button type="button" className={handRaised ? styles.actionActive : undefined} onClick={toggleHand} aria-pressed={handRaised}>
          <Hand aria-hidden="true" />{handRaised ? strings.meetLowerHand : strings.meetRaiseHand}
        </button>
        <div className={styles.reactions}>
          <button type="button" aria-label={strings.meetReact} title={strings.meetReact}><Smile aria-hidden="true" /></button>
          <div className={styles.reactionMenu}>
            {["👍", "👏", "❤️", "😂", "🎉"].map((emoji) => (
              <button type="button" key={emoji} onClick={() => react(emoji)} aria-label={`${strings.meetReact} ${emoji}`}>{emoji}</button>
            ))}
          </div>
        </div>
        <button type="button" onClick={() => void share()}>
          {copied ? <Copy aria-hidden="true" /> : <Share2 aria-hidden="true" />}
          {copied ? strings.meetLinkCopied : strings.meetInvite}
        </button>
      </div>
      {raisedHands.length > 0 && (
        <div className={styles.raisedHands} role="status"><Hand aria-hidden="true" />{strings.meetHandsRaised(raisedHands.join(", "))}</div>
      )}
      {reaction !== null && (
        <div key={reaction.key} className={styles.reactionBurst} role="status">
          <span>{reaction.emoji}</span><small>{reaction.name}</small>
        </div>
      )}
    </>
  );
}

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
  const [problem, setProblem] = useState<{ kind: "join" | "unavailable"; message: string } | null>(null);
  // Which camera and microphone, checked before joining rather than
  // discovered after. Somebody who joins broken usually leaves rather than
  // hunting for a settings menu mid-meeting.
  const [choices, setChoices] = useState<LocalUserChoices | null>(null);
  const [joinAttempt, setJoinAttempt] = useState(0);

  useEffect(() => {
    if (choices === null) return;
    let joined = true;
    setGrant(null);
    void (async () => {
      try {
        const g = await api.join(meetingId);
        // The person may have left before the token arrived; joining a room
        // nobody is looking at would hold a camera open.
        if (joined) setGrant(g);
      } catch (failure) {
        if (!joined) return;
        setProblem(failure instanceof MeetUnavailable
          ? { kind: "unavailable", message: strings.meetNoEngine }
          : { kind: "join", message: strings.meetJoinFailed });
      }
    })();
    return () => {
      joined = false;
    };
  }, [api, choices, joinAttempt, meetingId]);

  if (problem !== null) {
    return (
      <div className={styles.notice}>
        <div className={styles.noticeCard} role="alert">
          <span className={styles.noticeMark}>
            {problem.kind === "unavailable"
              ? <ServerOff aria-hidden="true" />
              : <Video aria-hidden="true" />}
          </span>
          <span className={styles.noticeEyebrow}>{strings.meetTitle}</span>
          <h1 className={styles.noticeTitle}>
            {problem.kind === "unavailable" ? strings.meetUnavailableTitle : strings.meetJoinProblemTitle}
          </h1>
          <p className={styles.noticeText}>{problem.message}</p>
          <div className={styles.noticeActions}>
            {problem.kind === "join" && (
              <Button
                icon={<RefreshCw aria-hidden="true" />}
                onClick={() => {
                  setProblem(null);
                  setJoinAttempt((attempt) => attempt + 1);
                }}
              >
                {strings.meetRetry}
              </Button>
            )}
            <Button variant="ghost" onClick={onLeft}>{strings.meetClose}</Button>
          </div>
        </div>
      </div>
    );
  }

  if (choices === null) {
    return (
      <div className={styles.room} data-lk-theme="default">
        <Button
          variant="ghost"
          className={styles.back}
          icon={<ArrowLeft aria-hidden="true" />}
          onClick={onLeft}
        >
          {strings.meetBack}
        </Button>
        <div className={styles.prejoin}>
          <PreJoin
            defaults={{
              // The same default the call itself uses: heard, and seen only
              // by choice.
              audioEnabled: true,
              videoEnabled: false,
              // LiveKit requires a non-empty display name before enabling its
              // submit button. alo's signed token owns the real identity, so
              // this internal placeholder is never shown or sent as identity.
              username: "alo",
            }}
            onSubmit={setChoices}
            onError={() => setProblem({ kind: "join", message: strings.meetJoinFailed })}
            joinLabel={strings.meetJoinNow}
            micLabel={strings.meetMicrophone}
            camLabel={strings.meetCamera}
            persistUserChoices
          />
        </div>
      </div>
    );
  }

  if (grant === null) {
    return (
      <div className={styles.notice} aria-live="polite" aria-busy="true">
        <span className={styles.joiningMark}><Video aria-hidden="true" /></span>
        <p className={styles.noticeText}>{strings.meetJoining}</p>
      </div>
    );
  }

  return (
    <div className={styles.room}>
      <MeetingHeader grant={grant} />
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
        <PresentingNotice />
        <MeetingActions meetingId={meetingId} />
      </LiveKitRoom>
      <Button
        variant="danger"
        className={styles.leave}
        onClick={onLeft}
        aria-label={strings.meetLeave}
        title={strings.meetLeave}
        icon={<PhoneOff aria-hidden="true" />}
      >
        {strings.meetLeave}
      </Button>
    </div>
  );
}
