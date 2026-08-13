// What this website has bought, and where each purchase got to (S2.15c3).
//
// Half of a domain purchase happens without anybody watching: once a payment
// settles, a background sweep registers the name and attaches it to the site.
// So this list is the recovery surface as much as the record — every row says
// what is happening now, what happens next, and, when something went wrong,
// the server's own sentence about it and what it means for the money.
//
// Two actions live here rather than only in the buy dialog, because closing a
// dialog must never strand a purchase: a `quoted` one can still be approved at
// the price shown in its own row, and one that has not been paid for can still
// be called off. Past payment there is no button, because there is no route —
// a refund is a conversation, and a button that pretended otherwise would be a
// promise nothing keeps.
import { useState } from "react";
import { Receipt, RefreshCw } from "lucide-react";

import { Button } from "../ds";
import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import { formatPrice } from "./catalogPricing";
import {
  canCancelPurchase,
  purchaseIsMoving,
  purchaseProgress,
  purchaseStateLabel,
} from "./domainPurchaseState";
import type { DomainQuote, SiteDomainPurchase } from "./types";
import styles from "./SitesModule.module.css";

/** Registrar prices are integer cents of a two-decimal currency. */
const CENTS = 2;

const stamp = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "short",
});

export function DomainPurchaseList({
  siteId,
  purchases,
  onUpdated,
  onRefresh,
}: {
  siteId: string;
  purchases: SiteDomainPurchase[];
  /** One purchase changed by an action here. */
  onUpdated: (purchase: SiteDomainPurchase) => void;
  /** Read the purchases again — what the machine states are watched with. */
  onRefresh: () => void;
}) {
  const api = useSitesApi();
  const [busyId, setBusyId] = useState<string | null>(null);
  const [armedId, setArmedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function approve(purchase: SiteDomainPurchase) {
    setBusyId(purchase.id);
    setError(null);
    try {
      // The row's own numbers — which are the ones on screen.
      const agreed: DomainQuote = {
        domain: purchase.domain,
        termYears: purchase.termYears,
        currency: purchase.currency,
        firstTermCents: purchase.firstTermCents,
        renewalCentsPerYear: purchase.renewalCentsPerYear,
        premium: purchase.premium,
      };
      onUpdated(await api.approveDomainPurchase(siteId, purchase.id, agreed));
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesDomainApproveFailed));
    } finally {
      setBusyId(null);
    }
  }

  async function callOff(purchase: SiteDomainPurchase) {
    if (armedId !== purchase.id) {
      setArmedId(purchase.id);
      return;
    }
    setBusyId(purchase.id);
    setError(null);
    try {
      onUpdated(await api.cancelDomainPurchase(siteId, purchase.id));
      setArmedId(null);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesDomainCancelFailed));
    } finally {
      setBusyId(null);
    }
  }

  return (
    <section className={styles.domainPanel} aria-labelledby="site-domain-purchases-title">
      <div className={styles.languagePanelIntro}>
        <span className={styles.languagePanelIcon} aria-hidden="true">
          <Receipt />
        </span>
        <div>
          <h2 id="site-domain-purchases-title" className={styles.languageTitle}>
            {strings.sitesDomainPurchases}
          </h2>
          <p className={styles.languageHint}>{strings.sitesDomainPurchasesHint}</p>
        </div>
        {purchases.some(purchaseIsMoving) && (
          <span className={styles.domainRowActions}>
            <Button
              variant="ghost"
              size="sm"
              icon={<RefreshCw size="var(--icon-size-inline)" />}
              onClick={onRefresh}
            >
              {strings.sitesDomainRefresh}
            </Button>
          </span>
        )}
      </div>

      {error !== null && (
        <p className={styles.publishError} role="alert">
          {error}
        </p>
      )}

      {purchases.length === 0 ? (
        <p className={styles.collaboratorEmpty}>{strings.sitesDomainPurchasesNone}</p>
      ) : (
        <div className={styles.domainRows}>
          {purchases.map((purchase) => (
            <article className={styles.domainRow} key={purchase.id}>
              <div className={styles.domainRowHead}>
                <span className={styles.mono}>{purchase.domain}</span>
                <span
                  className={
                    purchase.state === "configured"
                      ? `${styles.chip} ${styles.chipLive}`
                      : styles.chip
                  }
                >
                  {purchaseStateLabel(purchase.state)}
                </span>
                <span className={styles.domainRowActions}>
                  {purchase.state === "quoted" && (
                    <Button
                      size="sm"
                      disabled={busyId === purchase.id}
                      onClick={() => void approve(purchase)}
                    >
                      {strings.sitesDomainApproveAction(
                        formatPrice(
                          purchase.firstTermCents,
                          purchase.currency,
                          CENTS,
                        ),
                      )}
                    </Button>
                  )}
                  {canCancelPurchase(purchase) && (
                    <Button
                      variant={armedId === purchase.id ? "danger" : "ghost"}
                      size="sm"
                      disabled={busyId === purchase.id}
                      onClick={() => void callOff(purchase)}
                    >
                      {armedId === purchase.id
                        ? strings.sitesDomainCancelConfirm
                        : strings.sitesDomainCancel}
                    </Button>
                  )}
                </span>
              </div>

              <p className={styles.domainOfferPrice}>
                {strings.sitesDomainTermPrice(
                  formatPrice(purchase.firstTermCents, purchase.currency, CENTS),
                  purchase.termYears,
                )}
                {" · "}
                {strings.sitesDomainRenewalLine(
                  formatPrice(
                    purchase.renewalCentsPerYear,
                    purchase.currency,
                    CENTS,
                  ),
                )}
              </p>

              <p
                className={
                  purchase.state === "failed" ? styles.publishError : styles.hint
                }
                role={purchase.state === "failed" ? "alert" : undefined}
              >
                {purchaseProgress(purchase)}
              </p>

              {purchase.approvedAt !== null && (
                <p className={styles.hint}>
                  {strings.sitesDomainApprovedOn(
                    stamp.format(new Date(purchase.approvedAt)),
                  )}
                </p>
              )}
              {purchase.attempts > 1 && purchase.open && (
                <p className={styles.hint}>
                  {strings.sitesDomainAttempts(purchase.attempts)}
                </p>
              )}
            </article>
          ))}
        </div>
      )}
    </section>
  );
}
