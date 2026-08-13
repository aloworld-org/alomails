// The buy box (S2.15c3): type a name, see the complete address and what it
// really costs, and buy it without leaving the workspace.
//
// What this panel refuses to do is as much of the point as what it does.
//
//   * **It never prices anything itself.** Every number on screen came from
//     the seller in the answer being displayed. There is no arithmetic here —
//     not even multiplying a year by a term.
//   * **It never shows one price.** Wherever a first-term price appears, the
//     renewal price appears beside it, because the renewal is the half a bait
//     price hides in and a screen that omitted it would be the lie.
//   * **It never invents a rule.** A name that cannot be searched, an ending
//     alo does not sell, a term the registry refuses — all of those come back
//     from the server as sentences, and those sentences are what a person
//     reads, verbatim.
//
// A deployment that sells no domains — which is what production is until an
// ADR names a reseller — gets the connect-a-domain path instead of a buy box
// that fails at the price.
import { useCallback, useEffect, useState } from "react";
import { Search, ShoppingBag } from "lucide-react";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { SitesError, sitesMessage, useSitesApi } from "./api";
import { formatPrice } from "./catalogPricing";
import { DomainPurchaseDialog } from "./DomainPurchaseDialog";
import type {
  DomainCatalog,
  DomainEnding,
  DomainOffer,
  DomainSearchResult,
  SiteDomainPurchase,
} from "./types";
import styles from "./SitesModule.module.css";

/** Registrar prices are integer cents of a two-decimal currency. */
const CENTS = 2;

/** How long a typist is left alone before the seller is asked. Long enough
 *  that "acme" is one search rather than four, short enough that the answer
 *  feels like it belongs to what was typed. */
const SEARCH_PAUSE_MS = 400;

/** What to call an availability in the reader's language. */
function availabilityLabel(offer: DomainOffer): string {
  switch (offer.availability) {
    case "available":
      return strings.sitesDomainAvailable;
    case "taken":
      return strings.sitesDomainTaken;
    case "blocked":
      return strings.sitesDomainBlocked;
    case "unsupported":
      return strings.sitesDomainUnsupportedEnding;
  }
}

export function DomainBuyPanel({
  siteId,
  onPurchased,
}: {
  siteId: string;
  /** A newly approved purchase, so the record below the panel shows it
   *  without a second round trip. */
  onPurchased: (purchase: SiteDomainPurchase) => void;
}) {
  const api = useSitesApi();
  const [catalog, setCatalog] = useState<DomainCatalog | null>(null);
  // The server's own sentence for a deployment that sells no domains. Not an
  // error: it is the state production is in, and it has its own answer.
  const [unconfigured, setUnconfigured] = useState<string | null>(null);
  const [typed, setTyped] = useState("");
  const [result, setResult] = useState<DomainSearchResult | null>(null);
  const [searching, setSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [chosen, setChosen] = useState<DomainOffer | null>(null);

  useEffect(() => {
    let cancelled = false;
    api.domainCatalog().then(
      (answer) => {
        if (!cancelled) setCatalog(answer);
      },
      (reason: unknown) => {
        if (cancelled) return;
        if (reason instanceof SitesError && reason.reason === "unconfigured") {
          setUnconfigured(reason.detail ?? strings.sitesDomainUnconfiguredBody);
          return;
        }
        setError(sitesMessage(reason, strings.sitesDomainCatalogFailed));
      },
    );
    return () => {
      cancelled = true;
    };
  }, [api]);

  const search = useCallback(
    async (query: string, live: () => boolean) => {
      setSearching(true);
      try {
        const answer = await api.searchDomains(query, []);
        if (live()) {
          setResult(answer);
          setError(null);
        }
      } catch (reason) {
        if (live()) {
          setResult(null);
          setError(sitesMessage(reason, strings.sitesDomainSearchFailed));
        }
      } finally {
        if (live()) setSearching(false);
      }
    },
    [api],
  );

  useEffect(() => {
    const query = typed.trim();
    if (catalog === null || query === "") {
      setResult(null);
      setSearching(false);
      return;
    }
    let cancelled = false;
    const timer = setTimeout(() => {
      void search(query, () => !cancelled);
    }, SEARCH_PAUSE_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [catalog, search, typed]);

  /** The catalog entry for one offer's ending — the term limits and the
   *  registry's demands the dialog states before anybody types an address. */
  function endingOf(offer: DomainOffer): DomainEnding | null {
    const tail = offer.domain.slice(offer.domain.indexOf(".") + 1);
    return catalog?.endings.find((ending) => ending.tld === tail) ?? null;
  }

  if (unconfigured !== null) {
    return (
      <section className={styles.domainPanel} aria-labelledby="site-domain-buy-title">
        <div className={styles.languagePanelIntro}>
          <span className={styles.languagePanelIcon} aria-hidden="true">
            <ShoppingBag />
          </span>
          <div>
            <h2 id="site-domain-buy-title" className={styles.languageTitle}>
              {strings.sitesDomainUnconfiguredTitle}
            </h2>
            {/* The server's own sentence, which already names the way on. */}
            <p className={styles.languageHint}>{unconfigured}</p>
          </div>
        </div>
      </section>
    );
  }

  return (
    <section className={styles.domainPanel} aria-labelledby="site-domain-buy-title">
      <div className={styles.languagePanelIntro}>
        <span className={styles.languagePanelIcon} aria-hidden="true">
          <ShoppingBag />
        </span>
        <div>
          <h2 id="site-domain-buy-title" className={styles.languageTitle}>
            {strings.sitesDomainBuy}
          </h2>
          <p className={styles.languageHint}>{strings.sitesDomainBuyHint}</p>
        </div>
      </div>

      <div className={styles.domainSearchRow}>
        <label className={styles.domainAddField}>
          <span>{strings.sitesDomainSearchLabel}</span>
          <input
            className={styles.input}
            value={typed}
            placeholder={strings.sitesDomainSearchPlaceholder}
            disabled={catalog === null}
            onChange={(event) => setTyped(event.target.value)}
          />
        </label>
        <span className={styles.domainSearchStatus} role="status">
          {searching ? (
            <>
              <Spinner size={16} />
              {strings.sitesDomainSearching}
            </>
          ) : (
            <Search size="var(--icon-size-inline)" aria-hidden="true" />
          )}
        </span>
      </div>

      {error !== null && (
        <p className={styles.publishError} role="alert">
          {error}
        </p>
      )}

      {catalog !== null && !catalog.registrar.spendsMoney && (
        <p className={styles.hint}>
          {strings.sitesDomainTestRegistrar(catalog.registrar.name)}
        </p>
      )}
      {catalog !== null && !catalog.buyable && (
        <p className={styles.hint}>{strings.sitesDomainNotBuyable}</p>
      )}

      {result === null && error === null && (
        <p className={styles.collaboratorEmpty}>{strings.sitesDomainSearchInvite}</p>
      )}

      <div className={styles.domainRows}>
        {result?.offers.map((offer) => (
          <div className={styles.domainOffer} key={offer.domain}>
            <span className={styles.domainOfferName}>{offer.domain}</span>
            <span
              className={
                offer.availability === "available"
                  ? `${styles.chip} ${styles.chipLive}`
                  : styles.chip
              }
            >
              {availabilityLabel(offer)}
            </span>
            {offer.quote !== null && (
              <span className={styles.domainOfferPrice}>
                {strings.sitesDomainPriceLine(
                  formatPrice(offer.quote.firstTermCents, offer.quote.currency, CENTS),
                  formatPrice(
                    offer.quote.renewalCentsPerYear,
                    offer.quote.currency,
                    CENTS,
                  ),
                )}
                {offer.quote.premium && (
                  <span className={styles.badge}>{strings.sitesDomainPremium}</span>
                )}
              </span>
            )}
            {offer.availability === "available" && (
              <Button
                size="sm"
                disabled={!(result?.buyable ?? false)}
                onClick={() => setChosen(offer)}
              >
                {strings.sitesDomainChoose}
              </Button>
            )}
          </div>
        ))}
      </div>

      {catalog !== null && (
        <p className={styles.hint}>
          {strings.sitesDomainRegistrarLine(
            catalog.registrar.name,
            catalog.registrar.country.toUpperCase(),
          )}
        </p>
      )}

      {chosen !== null && (
        <DomainPurchaseDialog
          siteId={siteId}
          offer={chosen}
          ending={endingOf(chosen)}
          onClose={() => setChosen(null)}
          onApproved={(purchase) => {
            setChosen(null);
            onPurchased(purchase);
          }}
        />
      )}
    </section>
  );
}
