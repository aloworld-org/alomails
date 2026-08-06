// The invoice list: every document the tenant has raised, newest first, with
// what it is worth and where it stands.
//
// Two things it deliberately does not do. It does not compute money — every
// figure in the total column is the server's, carried on each list entry — and
// it does not decide what is overdue: `overdue` is computed by the server
// against its own date, so a browser with a wrong clock cannot clear or invent
// a late invoice.
//
// The status filter is a server-side one (`?status=`), not a filter over a
// loaded page: a bookkeeper asking for issued documents must get the tenant's
// issued documents, not the issued ones out of the first screenful.
import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { FileText } from "lucide-react";

import { Button, Spinner, cx } from "../ds";
import { strings, useLocale } from "../i18n";
import { billingMessage, useBillingApi } from "./api";
import { formatDocumentDate } from "./dates";
import { formatAmount } from "./money";
import { EmptyState, ErrorBanner } from "./parts";
import { DocumentChips } from "./status";
import type { BillingCustomer, BillingInvoiceSummary, InvoiceStatus } from "./types";
import styles from "./BillingModule.module.css";

/** The filter's choices, in the order a document moves through them. `all` is
 *  the absence of a filter, not a fifth status. */
const FILTERS = [
  { value: "all", label: () => strings.billingFilterAll },
  { value: "draft", label: () => strings.billingStatusDraft },
  { value: "issued", label: () => strings.billingStatusIssued },
  { value: "paid", label: () => strings.billingStatusPaid },
  { value: "void", label: () => strings.billingStatusVoid },
] as const;

type Filter = (typeof FILTERS)[number]["value"];

/** Whether a document answers the search box: its number, its customer's name
 *  or the customer's own reference. */
function matches(invoice: BillingInvoiceSummary, customer: string, needle: string): boolean {
  if (needle === "") return true;
  return [invoice.number ?? "", customer, invoice.reference]
    .join(" ")
    .toLowerCase()
    .includes(needle);
}

export function InvoicesView() {
  const api = useBillingApi();
  const locale = useLocale();
  const navigate = useNavigate();
  const [invoices, setInvoices] = useState<BillingInvoiceSummary[]>([]);
  const [customers, setCustomers] = useState<BillingCustomer[]>([]);
  const [filter, setFilter] = useState<Filter>("all");
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Archived customers are included: a document raised for a customer who has
  // since been archived still has to name them.
  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [list, people] = await Promise.all([
        api.invoices(filter === "all" ? undefined : (filter satisfies InvoiceStatus)),
        api.customers(true),
      ]);
      setInvoices(list);
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

  const names = useMemo(
    () => new Map(customers.map((c) => [c.id, c.name] as const)),
    [customers],
  );

  const shown = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return invoices.filter((i) => matches(i, names.get(i.customerId) ?? "", needle));
  }, [invoices, names, search]);

  return (
    <div className={styles.page}>
      <div className={styles.toolbar}>
        <input
          className={styles.search}
          type="search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={strings.billingSearchInvoices}
          aria-label={strings.billingSearchInvoices}
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
        <Button onClick={() => void navigate("new")}>{strings.billingNewInvoice}</Button>
      </div>

      {error !== null && <ErrorBanner message={error} />}

      {invoices.length === 0 && !loading && filter === "all" ? (
        <EmptyState
          Icon={FileText}
          title={strings.billingNoInvoicesTitle}
          body={strings.billingNoInvoicesBody}
          cta={strings.billingNewInvoice}
          onCta={() => void navigate("new")}
        />
      ) : shown.length === 0 && !loading ? (
        <p className={styles.noMatches}>{strings.billingNoMatches}</p>
      ) : (
        <div className={styles.tableWrap}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th scope="col">{strings.billingColNumber}</th>
                <th scope="col">{strings.billingColCustomer}</th>
                <th scope="col">{strings.billingColIssueDate}</th>
                <th scope="col">{strings.billingColDueDate}</th>
                <th scope="col">{strings.billingColStatus}</th>
                <th scope="col" className={styles.numeric}>
                  {strings.billingColTotal}
                </th>
              </tr>
            </thead>
            <tbody>
              {shown.map((invoice) => (
                <tr key={invoice.id} className={cx(invoice.overdue && styles.overdueRow)}>
                  <td>
                    <button
                      type="button"
                      className={cx(styles.rowName, styles.mono)}
                      onClick={() => void navigate(invoice.id)}
                    >
                      {invoice.number ?? strings.billingNotNumbered}
                    </button>
                  </td>
                  <td>{names.get(invoice.customerId) ?? strings.billingUnknownCustomer}</td>
                  <td>{formatDocumentDate(invoice.issueDate, locale, strings.billingNoDate)}</td>
                  <td>{formatDocumentDate(invoice.dueDate, locale, strings.billingNoDate)}</td>
                  <td className={styles.chips}>
                    <DocumentChips invoice={invoice} />
                  </td>
                  <td className={styles.numeric}>
                    {formatAmount(invoice.totals.grossCents, locale, invoice.currency)}
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
