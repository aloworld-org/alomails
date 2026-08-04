// The Accept / Maybe / Decline card shown in the reading pane when a message is
// a calendar invitation (an iMIP REQUEST; surfaced by the server as
// `alo:invitation`). Responding adds the event to the user's calendar (unless
// declining) and emails a reply to the organizer — both handled server-side via
// the message's blobId, so this component only needs the parsed summary.
import { useState } from "react";
import { CalendarCheck, Check, HelpCircle, X } from "lucide-react";

import { strings } from "../../i18n";
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

export function InvitationCard({ invitation, blobId }: Props) {
  const client = useJmapClient();
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<RsvpResponse | null>(null);
  const [error, setError] = useState(false);

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
    <div className={styles.card}>
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
          <button
            type="button"
            className={styles.accept}
            disabled={busy}
            onClick={() => void respond("accepted")}
          >
            <Check size={15} /> {strings.rsvpAccept}
          </button>
          <button
            type="button"
            className={styles.tentative}
            disabled={busy}
            onClick={() => void respond("tentative")}
          >
            <HelpCircle size={15} /> {strings.rsvpMaybe}
          </button>
          <button
            type="button"
            className={styles.decline}
            disabled={busy}
            onClick={() => void respond("declined")}
          >
            <X size={15} /> {strings.rsvpDecline}
          </button>
        </div>
      )}
      {error && (
        <div className={styles.error} role="alert">
          {strings.rsvpError}
        </div>
      )}
    </div>
  );
}
