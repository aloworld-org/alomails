// The create-site form: a name, and the address the site will live at. The
// address field carries a LIVE taken/free check against the server — the
// global namespace is the server's, so the form asks it rather than guessing
// — but the check is advisory: submitting is always allowed, and the server's
// refusal (taken, reserved, malformed) is shown with its own rule-naming
// sentence.
import { useEffect, useRef, useState } from "react";
import { Globe } from "lucide-react";

import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import { DialogFrame, Field } from "./parts";
import type { Site } from "./types";
import styles from "./SitesModule.module.css";

/** How long the address field stays still before the availability question is
 *  asked — long enough to skip mid-word states, short enough to feel live. */
export const SUBDOMAIN_CHECK_DELAY_MS = 350;

/** What the form currently knows about the typed address. */
type Check =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "available"; subdomain: string }
  | { kind: "taken"; subdomain: string }
  /** The server refused the label itself (syntax, reserved word) — its own
   *  sentence names the rule. */
  | { kind: "invalid"; message: string };

/** The check verdict line under the address field. */
function CheckLine({ check }: { check: Check }) {
  if (check.kind === "idle") return null;
  const text =
    check.kind === "checking"
      ? strings.sitesSubdomainChecking
      : check.kind === "available"
        ? strings.sitesSubdomainAvailable(check.subdomain)
        : check.kind === "taken"
          ? strings.sitesSubdomainTaken(check.subdomain)
          : check.message;
  const tone =
    check.kind === "available"
      ? styles.checkOk
      : check.kind === "checking"
        ? styles.checkPending
        : styles.checkBad;
  return (
    <p className={`${styles.checkLine} ${tone}`} role="status">
      {text}
    </p>
  );
}

export function NewSiteDialog({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (site: Site) => void;
}) {
  const api = useSitesApi();
  const [name, setName] = useState("");
  const [subdomain, setSubdomain] = useState("");
  const [check, setCheck] = useState<Check>({ kind: "idle" });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // The deployment-wide sites domain, for the "will live at" preview; null
  // while unknown (loading, or the config fetch failed) — the preview simply
  // stays off, the form works regardless.
  const [domain, setDomain] = useState<string | null>(null);
  // Answers can arrive out of order; only the newest question's answer counts.
  const checkSeq = useRef(0);

  useEffect(() => {
    let cancelled = false;
    api.config().then(
      (c) => {
        if (!cancelled && typeof c.domain === "string" && c.domain !== "") setDomain(c.domain);
      },
      () => {
        // Domain unknown: no preview line.
      },
    );
    return () => {
      cancelled = true;
    };
  }, [api]);

  useEffect(() => {
    const value = subdomain.trim().toLowerCase();
    const mine = ++checkSeq.current;
    if (value === "") {
      setCheck({ kind: "idle" });
      return;
    }
    setCheck({ kind: "checking" });
    const timer = setTimeout(() => {
      api
        .checkSubdomain(value)
        .then((r) => {
          if (checkSeq.current !== mine) return;
          setCheck(
            r.available
              ? { kind: "available", subdomain: r.subdomain }
              : { kind: "taken", subdomain: r.subdomain },
          );
        })
        .catch((err: unknown) => {
          if (checkSeq.current !== mine) return;
          setCheck({ kind: "invalid", message: sitesMessage(err, strings.sitesCheckFailed) });
        });
    }, SUBDOMAIN_CHECK_DELAY_MS);
    return () => clearTimeout(timer);
  }, [subdomain, api]);

  async function submit() {
    setBusy(true);
    try {
      onCreated(await api.createSite({ name: name.trim(), subdomain: subdomain.trim() }));
    } catch (err) {
      setError(sitesMessage(err, strings.sitesSaveFailed));
      setBusy(false);
    }
  }

  return (
    <DialogFrame
      Icon={Globe}
      title={strings.sitesNewSiteTitle}
      subtitle={strings.sitesNewSiteSubtitle}
      error={error}
      busy={busy}
      canSubmit={name.trim() !== "" && subdomain.trim() !== ""}
      submitLabel={strings.sitesCreateSite}
      onClose={onClose}
      onSubmit={() => void submit()}
    >
      <Field label={strings.sitesFieldName}>
        <input
          className={styles.input}
          value={name}
          onChange={(e) => setName(e.target.value)}
          autoFocus
        />
      </Field>
      <Field label={strings.sitesFieldSubdomain} hint={strings.sitesSubdomainHint}>
        <input
          className={styles.input}
          value={subdomain}
          onChange={(e) => setSubdomain(e.target.value)}
          autoCapitalize="none"
          autoCorrect="off"
          spellCheck={false}
        />
      </Field>
      <CheckLine check={check} />
      {domain !== null && subdomain.trim() !== "" && (
        <p className={styles.addressPreview}>
          {strings.sitesAddressPreview(`${subdomain.trim().toLowerCase()}.${domain}`)}
        </p>
      )}
    </DialogFrame>
  );
}
