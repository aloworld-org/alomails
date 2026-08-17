// What a document is worth: net, the VAT breakdown per rate, and gross.
//
// Every figure is the server's, straight off the document response. Nothing
// here adds anything up — not even the net of two lines — because a browser
// that computes a total is a browser that can disagree with the invoice the
// customer holds (`docs/design/billing.md`).
//
// `stale` says the draft has edits the server has not seen: the figures shown
// are then the last ones it computed, and they are dimmed and announced rather
// than silently passed off as current.
import { strings, useLocale } from "../i18n";
import { cx } from "../ds";
import { formatAmount, formatRate } from "./money";
import type { DocumentTotals } from "./types";
import styles from "./billingStyles";

export function TotalsPanel({
  totals,
  currency,
  stale,
}: {
  totals: DocumentTotals;
  currency: string;
  stale: boolean;
}) {
  const locale = useLocale();
  const amount = (cents: number) => formatAmount(cents, locale, currency);

  return (
    <section className={cx(styles.totals, stale && styles.stale)} aria-live="polite">
      <dl className={styles.totalsList}>
        <div className={styles.totalsRow}>
          <dt>{strings.billingTotalsNet}</dt>
          <dd className={styles.numeric}>{amount(totals.netCents)}</dd>
        </div>
        {totals.vatByRate.map((share) => (
          <div key={share.rateBp} className={styles.totalsRow}>
            <dt>{strings.billingVatAtRate(formatRate(share.rateBp, locale))}</dt>
            <dd className={styles.numeric}>{amount(share.vatCents)}</dd>
          </div>
        ))}
        <div className={cx(styles.totalsRow, styles.totalsGross)}>
          <dt>{strings.billingTotalsGross}</dt>
          <dd className={styles.numeric}>{amount(totals.grossCents)}</dd>
        </div>
      </dl>
      {stale && <p className={styles.totalsNote}>{strings.billingTotalsStale}</p>}
    </section>
  );
}
