// The quote list: every offer the tenant has made, newest first, with what it
// is worth and where it stands.
//
// It mirrors the invoice list deliberately, down to the server-side status
// filter: a salesperson asking for open offers must get the tenant's open
// offers, not the open ones out of the first screenful. And as there, no
// figure on this screen is computed here — the total is `totals.grossCents`
// off the list entry — and no date is judged here: `expired` is the server's,
// computed against its own date.
import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { FileSignature } from "lucide-react";

import { Button, Spinner, cx } from "../ds";
import { strings, useLocale } from "../i18n";
import { billingMessage, useBillingApi } from "./api";
import { formatDocumentDate } from "./dates";
import { formatAmount } from "./money";
import { BillingLoading, EmptyState, ErrorBanner } from "./parts";
import { QuoteChips } from "./status";
import type { BillingCustomer, BillingQuoteSummary, QuoteStatus } from "./types";
import styles from "./BillingModule.module.css";

/** The filter's choices, in the order an offer moves through them. `all` is
 *  the absence of a filter, not a sixth status. */
const FILTERS = [
  { value: "all", label: () => strings.billingFilterAll },
  { value: "draft", label: () => strings.billingStatusDraft },
  { value: "sent", label: () => strings.billingQuoteStatusSent },
  { value: "accepted", label: () => strings.billingQuoteStatusAccepted },
  { value: "declined", label: () => strings.billingQuoteStatusDeclined },
  { value: "expired", label: () => strings.billingQuoteStatusExpired },
] as const;

type Filter = (typeof FILTERS)[number]["value"];

/** Whether an offer answers the search box: its number, its customer's name or
 *  the customer's own reference. */
function matches(quote: BillingQuoteSummary, customer: string, needle: string): boolean {
  if (needle === "") return true;
  return [quote.number ?? "", customer, quote.reference].join(" ").toLowerCase().includes(needle);
}

export function QuotesView() {
  const api = useBillingApi();
  const locale = useLocale();
  const navigate = useNavigate();
  const [quotes, setQuotes] = useState<BillingQuoteSummary[]>([]);
  const [customers, setCustomers] = useState<BillingCustomer[]>([]);
  const [filter, setFilter] = useState<Filter>("all");
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Archived customers are included: an offer made to a customer who has since
  // been archived still has to name them.
  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [list, people] = await Promise.all([
        api.quotes(filter === "all" ? undefined : (filter satisfies QuoteStatus)),
        api.customers(true),
      ]);
      setQuotes(list);
      setCustomers(people);
      setError(null);
    } catch (err) {
      setError(billingMessage(err, strings.billingLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, filter]);

  useEffect(() => {
    void load();
  }, [load]);

  const names = useMemo(() => new Map(customers.map((c) => [c.id, c.name] as const)), [customers]);

  const shown = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return quotes.filter((q) => matches(q, names.get(q.customerId) ?? "", needle));
  }, [quotes, names, search]);

  return (
    <div className={styles.page}>
      <div className={styles.toolbar}>
        <input
          className={styles.search}
          type="search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={strings.billingSearchQuotes}
          aria-label={strings.billingSearchQuotes}
        />
        <label className={styles.toggle}>
          {strings.billingFilterStatus}
          <select
            className={styles.select}
            value={filter}
            onChange={(e) => setFilter(e.target.value as Filter)}
          >
            {FILTERS.map((f) => (
              <option key={f.value} value={f.value}>
                {f.label()}
              </option>
            ))}
          </select>
        </label>
        {loading && <Spinner size={16} />}
        {(quotes.length > 0 || filter !== "all") && (
          <Button onClick={() => void navigate("new")}>{strings.billingNewQuote}</Button>
        )}
      </div>

      {error !== null && <ErrorBanner message={error} />}

      {loading ? <BillingLoading /> : quotes.length === 0 && filter === "all" ? (
        <EmptyState
          Icon={FileSignature}
          title={strings.billingNoQuotesTitle}
          body={strings.billingNoQuotesBody}
          cta={strings.billingNewQuote}
          onCta={() => void navigate("new")}
        />
      ) : shown.length === 0 ? (
        <p className={styles.noMatches}>{strings.billingNoMatches}</p>
      ) : (
        <div className={styles.tableWrap}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th scope="col">{strings.billingColNumber}</th>
                <th scope="col">{strings.billingColCustomer}</th>
                <th scope="col">{strings.billingColSentDate}</th>
                <th scope="col">{strings.billingColValidUntil}</th>
                <th scope="col">{strings.billingColStatus}</th>
                <th scope="col" className={styles.numeric}>
                  {strings.billingColTotal}
                </th>
              </tr>
            </thead>
            <tbody>
              {shown.map((quote) => (
                <tr
                  key={quote.id}
                  className={cx(quote.status === "sent" && quote.expired && styles.overdueRow)}
                >
                  <td>
                    <button
                      type="button"
                      className={cx(styles.rowName, styles.mono)}
                      onClick={() => void navigate(quote.id)}
                    >
                      {quote.number ?? strings.billingNotNumbered}
                    </button>
                  </td>
                  <td>{names.get(quote.customerId) ?? strings.billingUnknownCustomer}</td>
                  <td>{formatDocumentDate(quote.sentDate, locale, strings.billingNoDate)}</td>
                  <td>{formatDocumentDate(quote.validUntil, locale, strings.billingNoDate)}</td>
                  <td className={styles.chips}>
                    <QuoteChips quote={quote} />
                  </td>
                  <td className={styles.numeric}>
                    {formatAmount(quote.totals.grossCents, locale, quote.currency)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
