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
    <section
      className="overflow-hidden rounded-2xl border border-subtle bg-surface shadow-sm"
      aria-labelledby="site-domains-title"
    >
      <div className="flex items-start gap-3 px-5 py-5 sm:px-6">
        <span
          className="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent"
          aria-hidden="true"
        >
          <Globe2 size={20} />
        </span>
        <div className="min-w-0">
          <h2
            id="site-domains-title"
            className="m-0 text-lg font-semibold text-text-primary"
          >
            {strings.sitesDomainOwned}
          </h2>
          <p className="m-0 mt-1 text-sm leading-6 text-text-secondary">
            {strings.sitesDomainOwnedHint}
          </p>
        </div>
      </div>

      <form
        className="flex flex-col gap-3 border-t border-subtle px-5 py-5 sm:flex-row sm:items-end sm:px-6"
        onSubmit={add}
      >
        <label className="flex min-w-0 flex-1 flex-col gap-1.5 text-sm font-semibold text-text-primary">
          <span>{strings.sitesDomainAddress}</span>
          <input
            className="min-h-11 w-full rounded-xl border border-subtle bg-surface px-3.5 text-base text-text-primary outline-none transition focus:border-accent focus:ring-2 focus:ring-accent-soft disabled:cursor-not-allowed disabled:opacity-60"
            value={typed}
            placeholder={strings.sitesDomainPlaceholder}
            autoComplete="url"
            disabled={busy}
            onChange={(event) => setTyped(event.target.value)}
          />
        </label>
        <Button type="submit" disabled={busy || typed.trim() === ""}>
          {strings.sitesDomainAdd}
        </Button>
      </form>

      {loading && (
        <div
          className="flex items-center gap-2 border-t border-subtle px-5 py-4 text-sm text-text-secondary sm:px-6"
          role="status"
        >
          <Spinner size={16} />
          {strings.sitesDomainsLoading}
        </div>
      )}
      {error !== null && (
        <p
          className="m-0 border-t border-danger/20 bg-danger/5 px-5 py-3 text-sm text-danger sm:px-6"
          role="alert"
        >
          {error}
        </p>
      )}
      {notice !== null && (
        <p
          className="m-0 border-t border-success/20 bg-success-tint px-5 py-3 text-sm text-success sm:px-6"
          role="status"
        >
          {notice}
        </p>
      )}

      {!loading && domains.length === 0 && (
        <p className="m-0 border-t border-subtle px-5 py-5 text-sm text-text-secondary sm:px-6">
          {strings.sitesDomainNoneBody}
        </p>
      )}

      <div className="divide-y divide-subtle border-t border-subtle">
        {domains.map((domain) => (
          <article
            className="flex flex-col gap-4 px-5 py-5 sm:px-6"
            key={domain.domain}
          >
            <div className="flex flex-wrap items-center gap-3">
              <strong className="min-w-0 flex-1 truncate font-mono text-sm text-text-primary sm:text-base">
                {domain.domain}
              </strong>
              <span
                className={
                  domain.status === "live"
                    ? "inline-flex min-h-7 items-center rounded-full bg-success-tint px-3 text-xs font-semibold text-success"
                    : "inline-flex min-h-7 items-center rounded-full bg-surface-raised px-3 text-xs font-semibold text-text-secondary"
                }
              >
                {statusLabel(domain)}
              </span>
              <span className="flex flex-wrap items-center gap-2">
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
              <p className="m-0 rounded-xl bg-danger/5 px-4 py-3 text-sm text-danger">
                {strings.sitesDomainRemoveHint}
              </p>
            )}

            {domain.status === "pending" && (
              <div className="rounded-xl border border-subtle bg-surface-raised p-4 sm:p-5">
                <p className="m-0 font-semibold text-text-primary">
                  {strings.sitesDomainRecordTitle}
                </p>
                <dl className="mt-4 grid gap-3">
                  {[
                    {
                      label: strings.sitesDomainRecordName,
                      value: domain.verifyRecord.name,
                    },
                    {
                      label: strings.sitesDomainRecordType,
                      value: domain.verifyRecord.type,
                    },
                    {
                      label: strings.sitesDomainRecordValue,
                      value: domain.verifyRecord.value,
                    },
                  ].map((field) => (
                    <div
                      className="grid gap-1 sm:grid-cols-[7rem_minmax(0,1fr)_auto] sm:items-center"
                      key={field.label}
                    >
                      <dt className="text-xs font-semibold uppercase tracking-wide text-text-secondary">
                        {field.label}
                      </dt>
                      <dd className="contents">
                        <code className="min-w-0 overflow-x-auto rounded-lg bg-surface px-3 py-2 font-mono text-sm text-text-primary">
                          {field.value}
                        </code>
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
                <p className="m-0 mt-4 text-sm leading-6 text-text-secondary">
                  {strings.sitesDomainRecordHint}
                </p>
              </div>
            )}

            {domain.status !== "pending" && siteHost !== null && (
              <p className="m-0 rounded-xl bg-surface-raised px-4 py-3 text-sm leading-6 text-text-secondary">
                {strings.sitesDomainPointHint(siteHost)}
              </p>
            )}
          </article>
        ))}
      </div>
    </section>
  );
}
