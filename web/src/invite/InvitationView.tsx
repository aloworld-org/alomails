// Claiming a workspace account from an invitation link.
//
// The first screen of alo anybody outside the admin console ever sees, and the
// one moment this person is present, proven by the token, and able to choose
// their own password and name an address that is not the mailbox they are
// about to depend on. Both are asked for here because there is no second
// chance: an account that can be signed into and never recovered is exactly
// what this replaces.
import { useEffect, useState, type FormEvent } from "react";
import { KeyRound, Check } from "lucide-react";
import { Link, useParams } from "react-router-dom";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { accept, invitation, type Invitation } from "./api";
import styles from "./InvitationView.module.css";

/** The floor the server enforces. Repeated here to say so before the round
 *  trip, never to decide it — the server refuses regardless. */
const MIN_PASSWORD = 8;

export function InvitationView() {
  const { token = "" } = useParams();
  const [invite, setInvite] = useState<Invitation | null>(null);
  const [password, setPassword] = useState("");
  const [recovery, setRecovery] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void invitation(token).then(
      (value) => {
        if (cancelled) return;
        setInvite(value);
        setLoading(false);
      },
      (reason: unknown) => {
        if (cancelled) return;
        setError(
          reason instanceof Error ? reason.message : strings.inviteLoadFailed,
        );
        setLoading(false);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [token]);

  async function submit(e: FormEvent) {
    e.preventDefault();
    if (password.length < MIN_PASSWORD || !recovery.includes("@")) return;
    setBusy(true);
    setError(null);
    try {
      await accept(token, password, recovery);
      setDone(true);
    } catch (reason: unknown) {
      setError(reason instanceof Error ? reason.message : strings.inviteFailed);
    } finally {
      setBusy(false);
    }
  }

  if (loading) {
    return (
      <div className={styles.wrap}>
        <Spinner />
      </div>
    );
  }

  // A refused link is the end of the road for this page: there is nothing to
  // retry and no form worth showing. The server's own sentence says whether it
  // was used or has expired without saying which invitations exist.
  if (invite === null) {
    return (
      <div className={styles.wrap}>
        <div className={styles.card}>
          <h1 className={styles.title}>{strings.inviteUnavailable}</h1>
          <p className={styles.body}>{error ?? strings.inviteLoadFailed}</p>
          <p className={styles.body}>{strings.inviteAskAdmin}</p>
        </div>
      </div>
    );
  }

  if (done) {
    return (
      <div className={styles.wrap}>
        <div className={styles.card}>
          <div className={styles.mark}>
            <Check strokeWidth={1.5} />
          </div>
          <h1 className={styles.title}>{strings.inviteDoneTitle}</h1>
          <p className={styles.body}>{strings.inviteDoneBody(invite.email)}</p>
          <Link to="/login" className={styles.action}>
            {strings.inviteGoToSignIn}
          </Link>
        </div>
      </div>
    );
  }

  const ready = password.length >= MIN_PASSWORD && recovery.includes("@");

  return (
    <div className={styles.wrap}>
      <form className={styles.card} onSubmit={(e) => void submit(e)}>
        <div className={styles.mark}>
          <KeyRound strokeWidth={1.5} />
        </div>
        <h1 className={styles.title}>{strings.inviteTitle}</h1>
        {/* The address is shown rather than editable: it is what the account
            was created as, and what the credential will be installed under. */}
        <p className={styles.body}>{strings.inviteFor(invite.email)}</p>

        <label className={styles.field}>
          <span className={styles.label}>{strings.invitePassword}</span>
          <input
            className={styles.input}
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete="new-password"
            autoFocus
          />
          <span className={styles.hint}>{strings.invitePasswordHint}</span>
        </label>

        <label className={styles.field}>
          <span className={styles.label}>{strings.inviteRecovery}</span>
          <input
            className={styles.input}
            type="email"
            value={recovery}
            onChange={(e) => setRecovery(e.target.value)}
            placeholder={strings.inviteRecoveryPlaceholder}
            autoComplete="email"
          />
          {/* Why it is asked for, in the place it is asked. Somebody typing
              their own address into a stranger's form deserves the reason. */}
          <span className={styles.hint}>{strings.inviteRecoveryHint}</span>
        </label>

        {error !== null && (
          <p className={styles.error} role="alert">
            {error}
          </p>
        )}

        <Button type="submit" disabled={!ready || busy}>
          {busy ? strings.inviteWorking : strings.inviteSubmit}
        </Button>
      </form>
    </div>
  );
}
