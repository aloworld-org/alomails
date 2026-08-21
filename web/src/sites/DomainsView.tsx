// Every address this website can be reached at, in one screen (S2.15c3):
// the alo address it always has, the domains its owner already owns, the buy
// box, and the record of what has been bought.
//
// The order is the order of the decision. A website is already reachable, so
// its own address is stated first and nothing here is presented as a repair.
// Connecting a domain somebody already owns comes next, because it is free,
// works on every deployment, and is what most people arrive wanting. Buying
// one is last, and on a deployment that sells none it is replaced by the
// server's own sentence saying so — not by a broken buy box.
import { useCallback, useEffect, useState } from "react";
import { ArrowLeft, ExternalLink, Globe2 } from "lucide-react";
import { Link, useParams } from "react-router-dom";

import { Spinner } from "../ds";
import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import { ConnectedDomains } from "./ConnectedDomains";
import { DomainBuyPanel } from "./DomainBuyPanel";
import { DomainPurchaseList } from "./DomainPurchaseList";
import { ErrorBanner } from "./parts";
import type { SiteDetail, SiteDomainPurchase } from "./types";

export function DomainsView() {
  const { siteId = "" } = useParams();
  const api = useSitesApi();
  const [site, setSite] = useState<SiteDetail | null>(null);
  const [purchases, setPurchases] = useState<SiteDomainPurchase[]>([]);
  // The deployment-wide sites domain; null while unknown, and the copy that
  // needs it stays off rather than naming a host this screen guessed.
  const [domain, setDomain] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadPurchases = useCallback(async () => {
    try {
      setPurchases(await api.domainPurchases(siteId));
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesDomainPurchasesLoadFailed));
    }
  }, [api, siteId]);

  const load = useCallback(async () => {
    setLoading(true);
    let manager = false;
    try {
      const detail = await api.site(siteId);
      setSite(detail);
      manager = detail.canManageCollaborators;
      setError(null);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesSiteLoadFailed));
    } finally {
      setLoading(false);
    }
    // Buying is the owner's, not the site editor's — the same fact the server
    // guards this surface with, so a restricted editor is not shown a panel
    // whose every request would come back 403.
    if (manager) await loadPurchases();
  }, [api, loadPurchases, siteId]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    let cancelled = false;
    api.config().then(
      (config) => {
        if (
          !cancelled &&
          typeof config.domain === "string" &&
          config.domain !== ""
        )
          setDomain(config.domain);
      },
      () => {
        // Domain unknown: everything here still works, the alo address line
        // simply stays off.
      },
    );
    return () => {
      cancelled = true;
    };
  }, [api]);

  const host =
    site !== null && domain !== null ? `${site.subdomain}.${domain}` : null;

  /** One purchase changed by an action, folded into the list in place — and
   *  appended when it is one this screen has not seen before. */
  function absorb(purchase: SiteDomainPurchase) {
    setPurchases((rows) =>
      rows.some((row) => row.id === purchase.id)
        ? rows.map((row) => (row.id === purchase.id ? purchase : row))
        : [purchase, ...rows],
    );
  }

  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-5 px-4 py-5 sm:px-6 sm:py-7 lg:px-8">
      <header className="flex min-h-12 flex-wrap items-center gap-3">
        <Link
          to=".."
          relative="path"
          className="inline-flex min-h-10 items-center gap-2 rounded-xl px-3 font-medium text-text-secondary no-underline transition-colors hover:bg-surface-raised hover:text-text-primary"
        >
          <ArrowLeft size={18} aria-hidden="true" />
          {strings.sitesBackToSite}
        </Link>
        <div className="min-w-0 flex-1">
          <h1 className="m-0 text-2xl font-bold tracking-tight text-text-primary sm:text-3xl">
            {strings.sitesDomains}
          </h1>
          {site !== null && (
            <p className="m-0 mt-0.5 truncate text-sm text-text-secondary">
              {site.name}
            </p>
          )}
        </div>
        {loading && <Spinner size={16} />}
      </header>

      {error !== null && <ErrorBanner message={error} />}

      {host !== null && (
        <section className="flex flex-col gap-4 rounded-2xl border border-subtle bg-surface px-5 py-5 shadow-sm sm:flex-row sm:items-center sm:justify-between sm:px-6">
          <div className="flex min-w-0 items-center gap-3">
            <span
              className="grid size-10 shrink-0 place-items-center rounded-xl bg-success-tint text-success"
              aria-hidden="true"
            >
              <Globe2 size={20} />
            </span>
            <div className="min-w-0">
              <p className="m-0 text-sm text-text-secondary">
                {strings.sitesDomainAloAddress}
              </p>
              <strong className="block truncate font-mono text-sm text-text-primary sm:text-base">
                {host}
              </strong>
            </div>
          </div>
          <a
            href={`https://${host}`}
            target="_blank"
            rel="noreferrer"
            className="inline-flex min-h-10 shrink-0 items-center justify-center gap-2 rounded-xl border border-subtle bg-surface px-4 font-semibold text-text-primary no-underline transition-colors hover:bg-surface-raised"
          >
            {strings.sitesPreview}
            <ExternalLink size={16} aria-hidden="true" />
          </a>
        </section>
      )}

      {site !== null && (
        <div className="flex flex-col gap-5">
          <ConnectedDomains siteId={site.id} siteHost={host} />
          {site.canManageCollaborators ? (
            <details className="group rounded-2xl border border-subtle bg-surface shadow-sm">
              <summary className="flex min-h-16 cursor-pointer list-none items-center justify-between gap-3 rounded-2xl px-5 py-3 font-semibold text-text-primary marker:content-none hover:bg-surface-raised sm:px-6">
                <span>{strings.sitesDomainBuy}</span>
                <span
                  className="text-xl font-normal text-text-secondary transition-transform group-open:rotate-45"
                  aria-hidden="true"
                >
                  +
                </span>
              </summary>
              <div className="flex flex-col gap-5 border-t border-subtle p-5 sm:p-6">
                <DomainBuyPanel siteId={site.id} onPurchased={absorb} />
                <DomainPurchaseList
                  siteId={site.id}
                  purchases={purchases}
                  onUpdated={absorb}
                  onRefresh={() => void loadPurchases()}
                />
              </div>
            </details>
          ) : (
            <p className="m-0 rounded-xl border border-subtle bg-surface-raised px-4 py-3 text-sm text-text-secondary">
              {strings.sitesDomainOwnerOnly}
            </p>
          )}
        </div>
      )}
    </div>
  );
}
