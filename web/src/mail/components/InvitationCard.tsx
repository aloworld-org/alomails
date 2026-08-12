// The Accept / Maybe / Decline card shown in the reading pane when a message is
// a calendar invitation (an iMIP REQUEST; surfaced by the server as
// `alo:invitation`). Responding adds the event to the user's calendar (unless
// declining) and emails a reply to the organizer — both handled server-side via
// the message's blobId, so this component only needs the parsed summary.
import { useEffect, useRef, useState } from "react";
import { CalendarCheck, CalendarX, Check, HelpCircle, X } from "lucide-react";

import { strings } from "../../i18n";
import { Button, Card } from "../../ds";
import { useJmapClient } from "../../jmap/useJmapClient";
import type { CalendarInvitation, RsvpResponse } from "../../jmap";
import styles from "./InvitationCard.module.css";

interface Props {
  invitation: CalendarInvitation;
  /** The invitation message's blobId — what the RSVP endpoint acts on. */
  blobId: string;
}

/** A human "when" line: an all-day date, or a date with a start–end time range. */
function whenLabel(inv: CalendarInvitation): string {
  const start = new Date(inv.startsAt);
  if (inv.allDay) {
    return start.toLocaleDateString(undefined, {
      weekday: "long",
      year: "numeric",
      month: "long",
      day: "numeric",
    });
  }
  const end = new Date(inv.endsAt);
  const date = start.toLocaleDateString(undefined, {
    weekday: "long",
    month: "long",
    day: "numeric",
  });
  const time = (d: Date) => d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
  return `${date} · ${time(start)} – ${time(end)}`;
}

function doneLabel(r: RsvpResponse): string {
  if (r === "accepted") return strings.rsvpAccepted;
  if (r === "declined") return strings.rsvpDeclined;
  return strings.rsvpTentative;
}

/** The cancellation notice: shown when the organizer withdrew the event. It
 *  removes the event from the calendar on mount (once) and reports the result. */
function CancellationCard({ invitation, blobId }: Props) {
  const client = useJmapClient();
  const applied = useRef(false);
  const [state, setState] = useState<"working" | "removed" | "absent" | "error">("working");

  useEffect(() => {
    if (applied.current) return; // guard React 18 double-invoke
    applied.current = true;
    void (async () => {
      try {
        const { removed } = await client.cancelInvitation(blobId);
        setState(removed ? "removed" : "absent");
      } catch {
        setState("error");
      }
    })();
  }, [client, blobId]);

  return (
    <Card pad="sm" className={styles.invitation}>
      <div className={styles.head}>
        <CalendarX size={20} className={styles.icon} aria-hidden="true" />
        <div className={styles.info}>
          <div className={styles.title}>
            {strings.cancelledTitle} {invitation.summary}
          </div>
          <div className={styles.when}>{whenLabel(invitation)}</div>
          <div className={styles.meta}>
            {state === "removed" && strings.cancelledRemoved}
            {state === "absent" && strings.cancelledAbsent}
            {state === "error" && strings.rsvpError}
          </div>
        </div>
      </div>
    </Card>
  );
}

/** A guest's reply, shown on the organizer's copy. Records the response on the
 *  event on mount (once) and reports whose reply it was. */
function ReplyCard({ invitation, blobId }: Props) {
  const client = useJmapClient();
  const applied = useRef(false);
  const [state, setState] = useState<"working" | "applied" | "other" | "error">("working");

  useEffect(() => {
    if (applied.current) return; // guard React 18 double-invoke
    applied.current = true;
    void (async () => {
      try {
        const { applied: ok } = await client.applyReply(blobId);
        setState(ok ? "applied" : "other");
      } catch {
        setState("error");
      }
    })();
  }, [client, blobId]);

  const who = invitation.attendee ?? strings.rsvpFrom;
  const verb =
    invitation.partstat != null ? doneLabel(invitation.partstat) : strings.replyResponded;

  return (
    <Card pad="sm" className={styles.invitation}>
      <div className={styles.head}>
        <CalendarCheck size={20} className={styles.icon} aria-hidden="true" />
        <div className={styles.info}>
          <div className={styles.title}>{invitation.summary}</div>
          <div className={styles.when}>{whenLabel(invitation)}</div>
          <div className={styles.meta}>{strings.replyFrom(who, verb)}</div>
          <div className={styles.meta}>
            {state === "applied" && strings.replyApplied}
            {state === "error" && strings.rsvpError}
          </div>
        </div>
      </div>
    </Card>
  );
}

export function InvitationCard({ invitation, blobId }: Props) {
  const client = useJmapClient();
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<RsvpResponse | null>(null);
  const [error, setError] = useState(false);

  if (invitation.method === "CANCEL") {
    return <CancellationCard invitation={invitation} blobId={blobId} />;
  }
  if (invitation.method === "REPLY") {
    return <ReplyCard invitation={invitation} blobId={blobId} />;
  }

  async function respond(response: RsvpResponse) {
    setBusy(true);
    setError(false);
    try {
      await client.rsvp(blobId, response);
      setDone(response);
    } catch {
      setError(true);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card pad="sm" className={styles.invitation}>
      <div className={styles.head}>
        <CalendarCheck size={20} className={styles.icon} aria-hidden="true" />
        <div className={styles.info}>
          <div className={styles.title}>{invitation.summary}</div>
          <div className={styles.when}>{whenLabel(invitation)}</div>
          {invitation.location != null && invitation.location !== "" && (
            <div className={styles.meta}>{invitation.location}</div>
          )}
          {invitation.organizer != null && (
            <div className={styles.meta}>
              {strings.rsvpFrom} {invitation.organizer}
            </div>
          )}
        </div>
      </div>

      {done !== null ? (
        <div className={styles.responded}>{doneLabel(done)}</div>
      ) : (
        <div className={styles.actions}>
          <Button
            icon={<Check />}
            disabled={busy}
            onClick={() => void respond("accepted")}
          >
            {strings.rsvpAccept}
          </Button>
          <Button
            variant="ghost"
            icon={<HelpCircle />}
            disabled={busy}
            onClick={() => void respond("tentative")}
          >
            {strings.rsvpMaybe}
          </Button>
          <Button
            variant="ghost"
            icon={<X />}
            disabled={busy}
            onClick={() => void respond("declined")}
          >
            {strings.rsvpDecline}
          </Button>
        </div>
      )}
      {error && (
        <div className={styles.error} role="alert">
          {strings.rsvpError}
        </div>
      )}
    </Card>
  );
}
