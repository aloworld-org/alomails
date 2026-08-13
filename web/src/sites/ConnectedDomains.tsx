// Connecting a domain the tenant already owns (S1.25a/b, given its screen in
// S2.15c3) — the path that works on every deployment, including the ones that
// sell no domains at all.
//
// Three things this panel must keep doing, because each is where somebody
// otherwise gets stuck for an afternoon:
//
//   * **The proof is shown, not described.** The TXT record's name, type and
//     value are the server's own three strings, copyable one by one. A screen
//     that composed them from a token would be a second, weaker copy of the
//     rule the verifier checks.
//   * **"Not found yet" is not a failure.** DNS takes minutes to travel, so a
//     check that finds nothing is a normal answer with a sentence saying to
//     try again shortly — never a red error over something nobody did wrong.
//   * **The last step is said out loud.** Proving ownership is not pointing
//     the domain at anything; the CNAME line is the difference between a
//     verified claim and a website people can reach.
import { useCallback, useEffect, useState, type FormEvent } from "react";
import { Copy, Globe2, RefreshCw, Trash2 } from "lucide-react";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import type { SiteDomain } from "./types";
import styles from "./SitesModule.module.css";

/** What to call a claim's status in the reader's language. */
function statusLabel(domain: SiteDomain): string {
  switch (domain.status) {
    case "pending":
      return strings.sitesDomainStatusPending;
    case "verified":
      return strings.sitesDomainStatusVerified;
    case "live":
      return strings.sitesDomainStatusLive;
  }
}

export function ConnectedDomains({
  siteId,
  siteHost,
}: {
  siteId: string;
  /** The website's alo address, which a connected domain is pointed at. Null
   *  while the deployment domain is unknown; the CNAME line then stays off
   *  rather than naming a host this screen guessed. */
  siteHost: string | null;
}) {
  const api = useSitesApi();
  const [domains, setDomains] = useState<SiteDomain[]>([]);
  const [typed, setTyped] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [armed, setArmed] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setDomains(await api.siteDomains(siteId));
      setError(null);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesDomainsLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, siteId]);

  useEffect(() => {
    void load();
  }, [load]);

  async function add(event: FormEvent) {
    event.preventDefault();
    const wanted = typed.trim();
    if (wanted === "") return;
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await api.addSiteDomain(siteId, wanted);
      setTyped("");
      await load();
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesDomainAddFailed));
    } finally {
      setBusy(false);
    }
  }

  async function check(domain: SiteDomain) {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const answer = await api.verifySiteDomain(siteId, domain.domain);
      setDomains((rows) =>
        rows.map((row) => (row.domain === answer.domain ? answer : row)),
      );
      // A record that has not travelled yet is the ordinary answer, and the
      // claim comes back exactly as it went in.
      setNotice(
        answer.status === "pending"
          ? strings.sitesDomainNotYet
          : strings.sitesDomainVerifiedNow(answer.domain),
      );
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesDomainVerifyFailed));
    } finally {
      setBusy(false);
    }
  }

  async function remove(domain: SiteDomain) {
    if (armed !== domain.domain) {
      setArmed(domain.domain);
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await api.removeSiteDomain(siteId, domain.domain);
      setArmed(null);
      await load();
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesDomainRemoveFailed));
    } finally {
      setBusy(false);
    }
  }

  async function copy(value: string) {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(value);
      setError(null);
    } catch {
      // A clipboard the browser refuses is not worth an alarm: the value is
      // on screen and selectable.
      setCopied(null);
    }
  }

  return (
    <section className={styles.domainPanel} aria-labelledby="site-domains-title">
      <div className={styles.languagePanelIntro}>
        <span className={styles.languagePanelIcon} aria-hidden="true">
          <Globe2 />
        </span>
        <div>
          <h2 id="site-domains-title" className={styles.languageTitle}>
            {strings.sitesDomainOwned}
          </h2>
          <p className={styles.languageHint}>{strings.sitesDomainOwnedHint}</p>
        </div>
      </div>

      <form className={styles.domainAddRow} onSubmit={add}>
        <label className={styles.domainAddField}>
          <span>{strings.sitesDomainAddress}</span>
          <input
            className={styles.input}
            value={typed}
            placeholder={strings.sitesDomainPlaceholder}
            autoComplete="url"
            disabled={busy}
            onChange={(event) => setTyped(event.target.value)}
          />
        </label>
        <Button type="submit" size="sm" disabled={busy || typed.trim() === ""}>
          {strings.sitesDomainAdd}
        </Button>
      </form>

      {loading && (
        <div className={styles.collaboratorStatus} role="status">
          <Spinner size={16} />
          {strings.sitesDomainsLoading}
        </div>
      )}
      {error !== null && (
        <p className={styles.publishError} role="alert">
          {error}
        </p>
      )}
      {notice !== null && (
        <p className={styles.collaboratorNotice} role="status">
          {notice}
        </p>
      )}

      {!loading && domains.length === 0 && (
        <p className={styles.collaboratorEmpty}>{strings.sitesDomainNoneBody}</p>
      )}

      <div className={styles.domainRows}>
        {domains.map((domain) => (
          <article className={styles.domainRow} key={domain.domain}>
            <div className={styles.domainRowHead}>
              <span className={styles.mono}>{domain.domain}</span>
              <span
                className={
                  domain.status === "live"
                    ? `${styles.chip} ${styles.chipLive}`
                    : styles.chip
                }
              >
                {statusLabel(domain)}
              </span>
              <span className={styles.domainRowActions}>
                {domain.status === "pending" && (
                  <Button
                    variant="ghost"
                    size="sm"
                    icon={<RefreshCw size="var(--icon-size-inline)" />}
                    disabled={busy}
                    onClick={() => void check(domain)}
                  >
                    {strings.sitesDomainCheck}
                  </Button>
                )}
                <Button
                  variant={armed === domain.domain ? "danger" : "ghost"}
                  size="sm"
                  icon={<Trash2 size="var(--icon-size-inline)" />}
                  disabled={busy}
                  onClick={() => void remove(domain)}
                >
                  {armed === domain.domain
                    ? strings.sitesDomainRemoveConfirm
                    : strings.sitesDomainRemove}
                </Button>
              </span>
            </div>

            {armed === domain.domain && (
              <p className={styles.hint}>{strings.sitesDomainRemoveHint}</p>
            )}

            {domain.status === "pending" && (
              <div className={styles.domainRecord}>
                <p className={styles.domainRecordTitle}>
                  {strings.sitesDomainRecordTitle}
                </p>
                <dl className={styles.domainRecordFields}>
                  {[
                    { label: strings.sitesDomainRecordName, value: domain.verifyRecord.name },
                    { label: strings.sitesDomainRecordType, value: domain.verifyRecord.type },
                    { label: strings.sitesDomainRecordValue, value: domain.verifyRecord.value },
                  ].map((field) => (
                    <div key={field.label}>
                      <dt>{field.label}</dt>
                      <dd>
                        <code className={styles.mono}>{field.value}</code>
                        <Button
                          variant="ghost"
                          size="sm"
                          icon={<Copy size="var(--icon-size-inline)" />}
                          onClick={() => void copy(field.value)}
                        >
                          {copied === field.value
                            ? strings.sitesDomainCopied
                            : strings.sitesDomainCopy}
                        </Button>
                      </dd>
                    </div>
                  ))}
                </dl>
                <p className={styles.hint}>{strings.sitesDomainRecordHint}</p>
              </div>
            )}

            {domain.status !== "pending" && siteHost !== null && (
              <p className={styles.hint}>{strings.sitesDomainPointHint(siteHost)}</p>
            )}
          </article>
        ))}
      </div>
    </section>
  );
}
