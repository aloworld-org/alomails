import { useEffect, useState, type FormEvent } from "react";
import { Check, Globe2, KeyRound } from "lucide-react";
import { Link, useParams } from "react-router-dom";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { acceptSiteInvitation, siteInvitation, sitesMessage } from "./api";
import type { SiteInvitation } from "./types";
import styles from "./SitesModule.module.css";

export function SiteInvitationView() {
  const { token = "" } = useParams();
  const [invitation, setInvitation] = useState<SiteInvitation | null>(null);
  const [password, setPassword] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void siteInvitation(token).then(
      (value) => {
        if (!cancelled) {
          setInvitation(value);
          setError(null);
          setLoading(false);
        }
      },
      (reason: unknown) => {
        if (!cancelled) {
          setError(sitesMessage(reason, strings.sitesInvitationLoadFailed));
          setLoading(false);
        }
      },
    );
    return () => {
      cancelled = true;
    };
  }, [token]);

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (password !== confirmation) {
      setError(strings.sitesInvitationPasswordMismatch);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const accepted = await acceptSiteInvitation(token, password);
      setInvitation(accepted);
      setDone(true);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesInvitationAcceptFailed));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className={styles.invitationPage}>
      <section className={styles.invitationCard} aria-labelledby="invitation-title">
        <span className={styles.invitationMark} aria-hidden="true">
          {done ? <Check /> : <Globe2 />}
        </span>
        {loading ? (
          <div className={styles.invitationLoading} role="status">
            <Spinner size={20} />
            <span>{strings.sitesInvitationLoading}</span>
          </div>
        ) : done && invitation !== null ? (
          <>
            <h1 id="invitation-title">{strings.sitesInvitationDone}</h1>
            <p>{strings.sitesInvitationDoneBody(invitation.email)}</p>
            <Link className={styles.invitationSignIn} to="/login">
              {strings.sitesInvitationSignIn}
            </Link>
          </>
        ) : invitation !== null ? (
          <>
            <h1 id="invitation-title">{strings.sitesInvitationHeading}</h1>
            <p>{strings.sitesInvitationSubtitle(invitation.siteName)}</p>
            <div className={styles.invitationIdentity}>
              <KeyRound aria-hidden="true" />
              <span>{invitation.email}</span>
            </div>
            <form className={styles.invitationForm} onSubmit={(event) => void submit(event)}>
              <label>
                <span>{strings.sitesInvitationPassword}</span>
                <input
                  className={styles.input}
                  type="password"
                  aria-label={strings.sitesInvitationPassword}
                  autoComplete="new-password"
                  value={password}
                  disabled={busy}
                  onChange={(event) => setPassword(event.target.value)}
                />
                <small>{strings.sitesInvitationPasswordHint}</small>
              </label>
              <label>
                <span>{strings.sitesInvitationConfirmPassword}</span>
                <input
                  className={styles.input}
                  type="password"
                  aria-label={strings.sitesInvitationConfirmPassword}
                  autoComplete="new-password"
                  value={confirmation}
                  disabled={busy}
                  onChange={(event) => setConfirmation(event.target.value)}
                />
              </label>
              {error !== null && (
                <p className={styles.invitationError} role="alert">
                  {error}
                </p>
              )}
              <Button
                type="submit"
                disabled={busy || password.length < 8 || confirmation.length < 8}
              >
                {busy ? strings.sitesInvitationAccepting : strings.sitesInvitationAccept}
              </Button>
            </form>
          </>
        ) : (
          <>
            <h1 id="invitation-title">{strings.sitesInvitationHeading}</h1>
            <p className={styles.invitationError} role="alert">
              {error ?? strings.sitesInvitationLoadFailed}
            </p>
            <Link className={styles.invitationSignIn} to="/login">
              {strings.sitesInvitationSignIn}
            </Link>
          </>
        )}
      </section>
    </main>
  );
}
