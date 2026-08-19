// The mail-import wizard, opened from the account menu. Pick a provider
// (Gmail/Outlook prefill the server + port; "Other" lets you type an IMAP
// host), enter your address and password, and pull recent mail into your
// Inbox. Talks to POST /import/imap via the JMAP client.
//
// Since D2.08 the frame and the fields are the design system's: `ds/Modal`
// brings the focus trap, Escape and the return of focus to whatever opened the
// dialog — none of which this module's own scrim had — and `ds/Field` binds
// each label to its control, which a bare `<label><span>…<input>` never did.
// The module's stylesheet is gone; what is left of it is the provider picker
// below, which is a radio group drawn as a segmented row and not a primitive
// `ds/` owns.
import { useId, useState } from "react";
import { Download, X } from "lucide-react";

import { strings } from "../i18n";
import { Button, Field, IconButton, Input, Modal } from "../ds";
import { useJmapClient } from "../jmap";

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

/** One of the three provider buttons. A radio group rather than a row of
 *  chips: exactly one is chosen and choosing another unchooses this one, which
 *  is what `role="radio"` says and what `ds/Chip`'s `pressed` — announced as a
 *  toggle button — does not. Kept local for that reason; the design system has
 *  no segmented control, and inventing one for a single caller is how the
 *  drift this queue is undoing started. */
const PROVIDER =
  "flex-1 px-3 py-2 rounded-md border text-sm font-medium " +
  "transition-colors duration-[var(--duration-fast)] ease-standard " +
  "focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-1";
const PROVIDER_REST = "border-subtle text-secondary hover:bg-raised";
const PROVIDER_ON = "border-accent text-primary bg-raised";

export function ImportModal({ onClose }: ImportModalProps) {
  const client = useJmapClient();
  const formId = useId();
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
  const effectiveHost =
    provider === "other" ? host.trim() : (PRESETS[provider]?.host ?? "");

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
    <Modal
      title={strings.importTitle}
      onClose={onClose}
      actions={
        <IconButton
          label={strings.importClose}
          icon={<X size={18} />}
          onClick={onClose}
        />
      }
      footer={
        <>
          <div className="flex-1" />
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={onClose}
            disabled={busy}
          >
            {strings.importClose}
          </Button>
          <Button
            type="submit"
            form={formId}
            size="sm"
            icon={<Download size={15} />}
            disabled={busy}
          >
            {busy ? strings.importRunning : strings.importStart}
          </Button>
        </>
      }
    >
      <form
        id={formId}
        className="flex flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          void run();
        }}
      >
        <p className="text-sm text-secondary">{strings.importIntro}</p>

        <div
          className="flex gap-2"
          role="radiogroup"
          aria-label={strings.importProvider}
        >
          {(["gmail", "outlook", "other"] as const).map((p) => (
            <button
              type="button"
              key={p}
              role="radio"
              aria-checked={provider === p}
              className={`${PROVIDER} ${provider === p ? PROVIDER_ON : PROVIDER_REST}`}
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
          // The port is 88px wide because a port is four digits; a grid rather
          // than two flex children so the two fields' labels line up whatever
          // either one is called in the reader's language.
          <div className="grid grid-cols-[1fr_88px] gap-3">
            <Field label={strings.importServer}>
              {(control) => (
                <Input
                  {...control}
                  value={host}
                  onChange={(e) => setHost(e.target.value)}
                  placeholder="imap.example.com"
                  autoComplete="off"
                />
              )}
            </Field>
            <Field label={strings.importPort}>
              {(control) => (
                <Input
                  {...control}
                  value={port}
                  onChange={(e) => setPort(e.target.value)}
                  inputMode="numeric"
                />
              )}
            </Field>
          </div>
        )}

        <Field label={strings.importEmail}>
          {(control) => (
            <Input
              {...control}
              type="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="you@gmail.com"
              autoComplete="username"
            />
          )}
        </Field>
        {/* The app-password sentence is the password field's hint rather than a
            loose paragraph under the form: it answers "what goes in this box"
            for Gmail and Outlook, and as a hint it is read out with the field
            instead of being left for somebody to find. */}
        <Field
          label={strings.importPassword}
          {...(provider === "other"
            ? {}
            : { hint: strings.importAppPasswordHint })}
        >
          {(control) => (
            <Input
              {...control}
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoComplete="off"
            />
          )}
        </Field>

        {error !== null && (
          <p className="text-sm text-danger" role="alert">
            {error}
          </p>
        )}
        {note !== null && (
          <p className="rounded-md bg-raised px-3 py-2 text-sm text-primary">
            {note}
          </p>
        )}
      </form>
    </Modal>
  );
}
