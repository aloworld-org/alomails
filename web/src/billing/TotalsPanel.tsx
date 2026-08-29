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
import { useQuoteTableOptions } from "./quoteTableOptions";
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
  const options = useQuoteTableOptions();
  const amount = (cents: number) =>
    `${formatAmount(cents, locale, currency)}${options.enabled && options.showCurrencyCode ? ` ${currency}` : ""}`;
  const showSummary = !options.enabled || options.totalsDetail !== "total";
  const showBreakdown =
    !options.enabled || options.totalsDetail === "breakdown";

  return (
    <section
      className={cx(
        styles.totals,
        stale && styles.stale,
        options.enabled &&
          options.totalsPlacement === "full" &&
          "!w-full !self-stretch max-sm:!w-full",
        options.enabled &&
          options.totalsPlacement === "footer" &&
          "!w-full !self-stretch !rounded-t-none !border-x-0 !border-b-0 !bg-transparent !px-0 max-sm:!rounded-xl max-sm:!border max-sm:!bg-raised/40 max-sm:!px-5",
        options.enabled && "max-sm:!w-full max-sm:!self-stretch",
        options.enabled &&
          options.totalsStyle === "minimal" &&
          "!rounded-none !border-0 !bg-transparent !px-0 !shadow-none",
        options.enabled &&
          options.totalsStyle === "framed" &&
          "!border-2 !border-primary/20 !bg-surface",
        options.enabled &&
          options.totalsStyle === "accent" &&
          "!border-accent/30 !bg-surface",
      )}
      aria-live="polite"
    >
      <dl className={styles.totalsList}>
        {showSummary && (
          <div className={styles.totalsRow}>
            <dt>{strings.billingTotalsNet}</dt>
            <dd className={styles.numeric}>{amount(totals.netCents)}</dd>
          </div>
        )}
        {showSummary &&
          (showBreakdown ? (
            totals.vatByRate.map((share) => (
              <div key={share.rateBp} className={styles.totalsRow}>
                <dt>
                  {strings.billingVatAtRate(formatRate(share.rateBp, locale))}
                </dt>
                <dd className={styles.numeric}>{amount(share.vatCents)}</dd>
              </div>
            ))
          ) : (
            <div className={styles.totalsRow}>
              <dt>{strings.billingColVat}</dt>
              <dd className={styles.numeric}>{amount(totals.vatCents)}</dd>
            </div>
          ))}
        <div
          className={cx(
            styles.totalsRow,
            (!options.enabled || options.emphasizeTotal) && styles.totalsGross,
            options.enabled &&
              options.totalsStyle === "accent" &&
              "!mt-2 !rounded-lg !border-0 !bg-accent !px-3 !py-2.5 !text-on-accent [&_dd]:!text-on-accent [&_dt]:!text-on-accent",
          )}
        >
          <dt>{strings.billingTotalsGross}</dt>
          <dd className={styles.numeric}>{amount(totals.grossCents)}</dd>
        </div>
      </dl>
      {options.enabled && options.showTaxNote && (
        <p className="mt-3 text-xs text-tertiary">
          {strings.billingVatSeparateNote}
        </p>
      )}
      {stale && (
        <p className={styles.totalsNote}>{strings.billingTotalsStale}</p>
      )}
    </section>
  );
}
