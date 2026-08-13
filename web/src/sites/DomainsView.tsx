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
import { ArrowLeft } from "lucide-react";
import { Link, useParams } from "react-router-dom";

import { Spinner } from "../ds";
import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import { ConnectedDomains } from "./ConnectedDomains";
import { DomainBuyPanel } from "./DomainBuyPanel";
import { DomainPurchaseList } from "./DomainPurchaseList";
import { ErrorBanner } from "./parts";
import type { SiteDetail, SiteDomainPurchase } from "./types";
import styles from "./SitesModule.module.css";

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
        if (!cancelled && typeof config.domain === "string" && config.domain !== "")
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

  const host = site !== null && domain !== null ? `${site.subdomain}.${domain}` : null;

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
    <div className={styles.page}>
      <header className={styles.header}>
        <Link to=".." relative="path" className={styles.backLink}>
          <ArrowLeft size="var(--icon-size-inline)" aria-hidden="true" />
          {strings.sitesBack}
        </Link>
        <div className={styles.siteHead}>
          <h1 className={styles.title}>{strings.sitesDomains}</h1>
          {site !== null && (
            <span className={styles.submissionSiteName}>{site.name}</span>
          )}
        </div>
        {loading && <Spinner size={16} />}
      </header>

      {error !== null && <ErrorBanner message={error} />}

      {host !== null && (
        <p className={styles.domainAloAddress}>
          {strings.sitesDomainAloAddress}{" "}
          <a href={`https://${host}`} target="_blank" rel="noreferrer" className={styles.mono}>
            {host}
          </a>
        </p>
      )}

      {site !== null && (
        <>
          <ConnectedDomains siteId={site.id} siteHost={host} />
          {site.canManageCollaborators ? (
            <>
              <DomainBuyPanel siteId={site.id} onPurchased={absorb} />
              <DomainPurchaseList
                siteId={site.id}
                purchases={purchases}
                onUpdated={absorb}
                onRefresh={() => void loadPurchases()}
              />
            </>
          ) : (
            <p className={styles.domainAloAddress}>{strings.sitesDomainOwnerOnly}</p>
          )}
        </>
      )}
    </div>
  );
}
