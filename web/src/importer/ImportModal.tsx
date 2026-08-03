// The mail-import wizard, opened from the account menu. Pick a provider
// (Gmail/Outlook prefill the server + port; "Other" lets you type an IMAP
// host), enter your address and password, and pull recent mail into your
// Inbox. Talks to POST /import/imap via the JMAP client.
import { useState } from "react";
import { Download, X } from "lucide-react";

import { strings } from "../i18n";
import { Button } from "../ds";
import { useJmapClient } from "../jmap";
import styles from "./ImportModal.module.css";

interface ImportModalProps {
  onClose: () => void;
}

type Provider = "gmail" | "outlook" | "other";

/** Known IMAP endpoints so the common cases need no server typing. */
const PRESETS: Record<Provider, { host: string; port: number } | null> = {
  gmail: { host: "imap.gmail.com", port: 993 },
  outlook: { host: "outlook.office365.com", port: 993 },
  other: null,
};

export function ImportModal({ onClose }: ImportModalProps) {
  const client = useJmapClient();
  const [provider, setProvider] = useState<Provider>("gmail");
  const [host, setHost] = useState("");
  const [port, setPort] = useState("993");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  function chooseProvider(next: Provider) {
    setProvider(next);
    const preset = PRESETS[next];
    if (preset) {
      setHost(preset.host);
      setPort(String(preset.port));
    } else {
      setHost("");
    }
  }

  // Gmail/Outlook use their preset host; "Other" uses the typed host.
  const effectiveHost = provider === "other" ? host.trim() : (PRESETS[provider]?.host ?? "");

  async function run() {
    setError(null);
    setNote(null);
    if (effectiveHost === "" || email.trim() === "" || password === "") {
      setError(strings.importNeedsFields);
      return;
    }
    setBusy(true);
    try {
      const result = await client.importImap({
        host: effectiveHost,
        port: Number(port) || 993,
        username: email.trim(),
        password,
      });
      setNote(strings.importDone(result.imported, result.skipped));
      setPassword("");
    } catch (e) {
      setError(e instanceof Error ? e.message : strings.importNeedsFields);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div
        className={styles.modal}
        role="dialog"
        aria-modal="true"
        aria-label={strings.importTitle}
        onClick={(e) => e.stopPropagation()}
      >
        <div className={styles.head}>
          <h2 className={styles.title}>{strings.importTitle}</h2>
          <button
            type="button"
            className={styles.close}
            onClick={onClose}
            aria-label={strings.importClose}
          >
            <X size={18} />
          </button>
        </div>

        <form
          className={styles.body}
          onSubmit={(e) => {
            e.preventDefault();
            void run();
          }}
        >
          <p className={styles.intro}>{strings.importIntro}</p>

          <div className={styles.providers} role="radiogroup" aria-label={strings.importProvider}>
            {(["gmail", "outlook", "other"] as const).map((p) => (
              <button
                type="button"
                key={p}
                role="radio"
                aria-checked={provider === p}
                className={provider === p ? styles.providerActive : styles.provider}
                onClick={() => chooseProvider(p)}
              >
                {p === "gmail"
                  ? strings.importProviderGmail
                  : p === "outlook"
                    ? strings.importProviderOutlook
                    : strings.importProviderOther}
              </button>
            ))}
          </div>

          {provider === "other" && (
            <div className={styles.serverRow}>
              <label className={styles.field}>
                <span>{strings.importServer}</span>
                <input
                  value={host}
                  onChange={(e) => setHost(e.target.value)}
                  placeholder="imap.example.com"
                  autoComplete="off"
                />
              </label>
              <label className={styles.portField}>
                <span>{strings.importPort}</span>
                <input
                  value={port}
                  onChange={(e) => setPort(e.target.value)}
                  inputMode="numeric"
                />
              </label>
            </div>
          )}

          <label className={styles.field}>
            <span>{strings.importEmail}</span>
            <input
              type="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="you@gmail.com"
              autoComplete="username"
            />
          </label>
          <label className={styles.field}>
            <span>{strings.importPassword}</span>
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoComplete="off"
            />
          </label>
          {provider !== "other" && <p className={styles.hint}>{strings.importAppPasswordHint}</p>}

          {error !== null && <p className={styles.error}>{error}</p>}
          {note !== null && <p className={styles.note}>{note}</p>}

          <div className={styles.actions}>
            <Button type="button" variant="ghost" size="sm" onClick={onClose} disabled={busy}>
              {strings.importClose}
            </Button>
            <Button type="submit" size="sm" icon={<Download size={15} />} disabled={busy}>
              {busy ? strings.importRunning : strings.importStart}
            </Button>
          </div>
        </form>
      </div>
    </div>
  );
}
