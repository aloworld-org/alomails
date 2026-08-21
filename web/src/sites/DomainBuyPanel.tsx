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
      <section
        className="rounded-2xl border border-subtle bg-surface-raised p-5 sm:p-6"
        aria-labelledby="site-domain-buy-title"
      >
        <div className="flex items-start gap-3">
          <span
            className="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent"
            aria-hidden="true"
          >
            <ShoppingBag size={20} />
          </span>
          <div className="min-w-0">
            <h2
              id="site-domain-buy-title"
              className="m-0 text-lg font-semibold text-text-primary"
            >
              {strings.sitesDomainUnconfiguredTitle}
            </h2>
            {/* The server's own sentence, which already names the way on. */}
            <p className="m-0 mt-1 text-sm leading-6 text-text-secondary">
              {unconfigured}
            </p>
          </div>
        </div>
      </section>
    );
  }

  return (
    <section
      className="flex flex-col gap-5 rounded-2xl bg-surface-raised p-5 sm:p-6"
      aria-labelledby="site-domain-buy-title"
    >
      <div className="flex items-start gap-3">
        <span
          className="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent"
          aria-hidden="true"
        >
          <ShoppingBag size={20} />
        </span>
        <div className="min-w-0">
          <h2
            id="site-domain-buy-title"
            className="m-0 text-lg font-semibold text-text-primary"
          >
            {strings.sitesDomainBuy}
          </h2>
          <p className="m-0 mt-1 text-sm leading-6 text-text-secondary">
            {strings.sitesDomainBuyHint}
          </p>
        </div>
      </div>

      <div className="flex flex-col gap-2 sm:flex-row sm:items-end">
        <label className="flex min-w-0 flex-1 flex-col gap-1.5 text-sm font-semibold text-text-primary">
          <span>{strings.sitesDomainSearchLabel}</span>
          <input
            className="min-h-11 w-full rounded-xl border border-subtle bg-surface px-3.5 text-base text-text-primary outline-none transition focus:border-accent focus:ring-2 focus:ring-accent-soft disabled:cursor-not-allowed disabled:bg-surface disabled:text-text-tertiary"
            value={typed}
            placeholder={strings.sitesDomainSearchPlaceholder}
            disabled={catalog === null}
            onChange={(event) => setTyped(event.target.value)}
          />
        </label>
        <span
          className="inline-flex min-h-11 min-w-11 items-center justify-center gap-2 rounded-xl bg-surface px-3 text-sm text-text-secondary"
          role="status"
        >
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
        <p
          className="m-0 rounded-xl border border-danger/20 bg-danger/5 px-4 py-3 text-sm text-danger"
          role="alert"
        >
          {error}
        </p>
      )}

      {catalog !== null && !catalog.registrar.spendsMoney && (
        <p className="m-0 text-sm leading-6 text-text-secondary">
          {strings.sitesDomainTestRegistrar(catalog.registrar.name)}
        </p>
      )}
      {catalog !== null && !catalog.buyable && (
        <p className="m-0 text-sm leading-6 text-text-secondary">
          {strings.sitesDomainNotBuyable}
        </p>
      )}

      {result === null && error === null && (
        <p className="m-0 rounded-xl border border-dashed border-subtle bg-surface px-4 py-5 text-center text-sm text-text-secondary">
          {strings.sitesDomainSearchInvite}
        </p>
      )}

      <div className="divide-y divide-subtle overflow-hidden rounded-xl border border-subtle bg-surface">
        {result?.offers.map((offer) => (
          <div
            className="grid gap-3 px-4 py-4 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center"
            key={offer.domain}
          >
            <div className="flex min-w-0 flex-wrap items-center gap-2">
              <strong className="truncate font-mono text-sm text-text-primary sm:text-base">
                {offer.domain}
              </strong>
              <span
                className={
                  offer.availability === "available"
                    ? "inline-flex min-h-7 items-center rounded-full bg-success-tint px-3 text-xs font-semibold text-success"
                    : "inline-flex min-h-7 items-center rounded-full bg-surface-raised px-3 text-xs font-semibold text-text-secondary"
                }
              >
                {availabilityLabel(offer)}
              </span>
            </div>
            <div className="flex flex-wrap items-center gap-3 sm:justify-end">
              {offer.quote !== null && (
                <span className="text-sm leading-5 text-text-secondary">
                  {strings.sitesDomainPriceLine(
                    formatPrice(
                      offer.quote.firstTermCents,
                      offer.quote.currency,
                      CENTS,
                    ),
                    formatPrice(
                      offer.quote.renewalCentsPerYear,
                      offer.quote.currency,
                      CENTS,
                    ),
                  )}
                  {offer.quote.premium && (
                    <span className="ml-2 inline-flex min-h-6 items-center rounded-full bg-accent-soft px-2 text-xs font-semibold text-accent">
                      {strings.sitesDomainPremium}
                    </span>
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
          </div>
        ))}
      </div>

      {catalog !== null && (
        <p className="m-0 text-xs leading-5 text-text-tertiary">
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
