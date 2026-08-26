// App-specific passwords (mail M1.3) — the Settings surface for the
// credentials legacy mail clients (IMAP/POP3/SMTP) sign in with. Create shows
// the secret exactly once with a copy affordance — the server keeps only a
// hash and cannot repeat it — the list shows name / created / last used, and
// revoke is immediate: the next connection with that password fails.
import { useCallback, useEffect, useState } from "react";
import type { FormEvent } from "react";
import { Check, Copy, X } from "lucide-react";

import { strings } from "../i18n";
import { Button, IconButton, Input, Spinner } from "../ds";
import { useJmapClient } from "../jmap";
import type { AppPassword } from "../jmap";
import styles from "./AppPasswordsSection.module.css";

/** A freshly created password, held only in memory until dismissed. */
interface FreshSecret {
  name: string;
  secret: string;
}

export function AppPasswordsSection() {
  const client = useJmapClient();
  const [passwords, setPasswords] = useState<AppPassword[] | null>(null);
  const [name, setName] = useState("");
  const [fresh, setFresh] = useState<FreshSecret | null>(null);
  const [copied, setCopied] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    void client
      .listAppPasswords()
      .then(setPasswords)
      .catch(() => setError(strings.appPasswordListError));
  }, [client]);
  useEffect(load, [load]);

  async function create(e: FormEvent) {
    e.preventDefault();
    if (name.trim().length === 0 || busy) return;
    setBusy(true);
    setError(null);
    try {
      const made = await client.createAppPassword(name.trim());
      setFresh({ name: made.name, secret: made.secret });
      setCopied(false);
      setName("");
      load();
    } catch {
      setError(strings.appPasswordCreateError);
    } finally {
      setBusy(false);
    }
  }

  async function revoke(id: string) {
    setBusy(true);
    setError(null);
    try {
      await client.revokeAppPassword(id);
      load();
    } catch {
      setError(strings.appPasswordRevokeError);
    } finally {
      setBusy(false);
    }
  }

  async function copy(secret: string) {
    try {
      await navigator.clipboard.writeText(secret);
      setCopied(true);
    } catch {
      // Clipboard access refused: the secret stays on screen to select by
      // hand, which is the affordance the card already is.
    }
  }

  const day = (iso: string) => new Date(iso).toLocaleDateString();

  return (
    <div className={styles.wrap}>
      {fresh !== null && (
        <div className={styles.secretCard} role="status">
          <span className={styles.secretName}>
            {strings.appPasswordSecretFor(fresh.name)}
          </span>
          {/* The one appearance the secret ever makes. `user-select: all` in
              the stylesheet makes a click select the whole thing for the
              clipboard-less path. */}
          <code className={styles.secret}>{fresh.secret}</code>
          <span className={styles.secretHint}>
            {strings.appPasswordSecretHint}
          </span>
          <span className={styles.secretActions}>
            <Button size="sm" onClick={() => void copy(fresh.secret)}>
              {copied ? (
                <>
                  <Check size={14} /> {strings.appPasswordCopied}
                </>
              ) : (
                <>
                  <Copy size={14} /> {strings.appPasswordCopy}
                </>
              )}
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setFresh(null)}>
              {strings.appPasswordSecretDone}
            </Button>
          </span>
        </div>
      )}

      {passwords === null ? (
        error === null && <Spinner size={18} />
      ) : passwords.length === 0 ? (
        <p className={styles.none}>{strings.appPasswordNone}</p>
      ) : (
        <ul className={styles.list}>
          {passwords.map((p) => (
            <li key={p.id} className={styles.row}>
              <span className={styles.rowName}>{p.name}</span>
              <span className={styles.rowDates}>
                {strings.appPasswordCreated(day(p.createdAt))}
                {" · "}
                {p.lastUsedAt === null
                  ? strings.appPasswordNeverUsed
                  : strings.appPasswordLastUsed(day(p.lastUsedAt))}
              </span>
              {/* The label names the row: five revoke buttons in a list are
                  five different actions, and a screen reader should say so. */}
              <IconButton
                label={strings.appPasswordRevokeFor(p.name)}
                icon={<X />}
                onClick={() => void revoke(p.id)}
                className={styles.revoke}
              />
            </li>
          ))}
        </ul>
      )}

      <form className={styles.addRow} onSubmit={create}>
        <Input
          className={styles.grow}
          value={name}
          onChange={(e) => setName(e.target.value)}
          maxLength={100}
          placeholder={strings.appPasswordNamePlaceholder}
          aria-label={strings.appPasswordNamePlaceholder}
        />
        {/* `type="submit"` said out loud: `ds/Button` defaults to "button". */}
        <Button type="submit" disabled={busy || name.trim().length === 0}>
          {strings.appPasswordCreate}
        </Button>
      </form>

      {error !== null && (
        <p className={styles.error} role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
