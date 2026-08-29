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
//
// The list is also where money is chased (B1.26): a late row carries the one
// click that writes the reminder for it. The letter is the server's, and it
// goes to the sender's own Drafts — this screen sends nothing, and says so.
import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { FileText } from "lucide-react";

import {
  Button,
  Input,
  Select,
  Spinner,
  Table,
  Td,
  Th,
  Toolbar,
  ToolbarSpacer,
  cx,
} from "../ds";
import { strings, useLocale } from "../i18n";
import { billingMessage, useBillingApi } from "./api";
import { BillingPagination } from "./BillingPagination";
import { formatDocumentDate } from "./dates";
import { formatAmount } from "./money";
import { BillingLoading, EmptyState, ErrorBanner } from "./parts";
import { DocumentChips } from "./status";
import type {
  BillingCustomer,
  BillingInvoiceSummary,
  InvoiceStatus,
} from "./types";
import styles from "./billingStyles";
import { useBillingPagination } from "./useBillingPagination";

/** The filter's choices, in the order a document moves through them.
 *
 *  `all` is the absence of a filter, not a fifth status — and `overdue` is not
 *  one either: it is a **view** over the issued documents (issued, past its
 *  date, not settled), which the server answers on its own route because it is
 *  judged against the server's date. Both sit in one control because that is
 *  the one question a bookkeeper is asking. */
const FILTERS = [
  { value: "all", label: () => strings.billingFilterAll },
  { value: "overdue", label: () => strings.billingFilterOverdue },
  { value: "draft", label: () => strings.billingStatusDraft },
  { value: "issued", label: () => strings.billingStatusIssued },
  { value: "paid", label: () => strings.billingStatusPaid },
  { value: "void", label: () => strings.billingStatusVoid },
] as const;

type Filter = (typeof FILTERS)[number]["value"];

/** The list read one filter choice asks for. Keeps the two server surfaces —
 *  the status filter and the overdue view — behind one call site, so the page
 *  never has to hold both in its head. */
function listFor(api: ReturnType<typeof useBillingApi>, filter: Filter) {
  if (filter === "overdue") return api.overdueInvoices();
  return api.invoices(
    filter === "all" ? undefined : (filter satisfies InvoiceStatus),
  );
}

/** Whether a document answers the search box: its number, its customer's name
 *  or the customer's own reference. */
function matches(
  invoice: BillingInvoiceSummary,
  customer: string,
  needle: string,
): boolean {
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
  // The document a reminder is being written for, and what the last one said.
  // Both are the page's, not a row's: only one reminder is written at a time,
  // and its answer is reported once above the list rather than in a cell.
  const [reminding, setReminding] = useState<string | null>(null);
  const [reminded, setReminded] = useState<string | null>(null);

  // Archived customers are included: a document raised for a customer who has
  // since been archived still has to name them.
  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [list, people] = await Promise.all([
        listFor(api, filter),
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
    return invoices.filter((i) =>
      matches(i, names.get(i.customerId) ?? "", needle),
    );
  }, [invoices, names, search]);
  const paged = useBillingPagination(shown, `${search}\u0000${filter}`);

  /** Chase one late invoice: the server writes the letter from the stored
   *  document and leaves it in the sender's Drafts. Nothing is sent, and the
   *  invoice is untouched — so the click needs no confirmation, and the list
   *  is not reloaded afterwards. A refusal (a settled document somebody else
   *  just paid, a customer with no address) is shown in the server's words. */
  async function remind(invoice: BillingInvoiceSummary) {
    setReminding(invoice.id);
    setReminded(null);
    setError(null);
    try {
      const draft = await api.remindInvoice(invoice.id);
      setReminded(
        strings.billingReminderDrafted(
          draft.invoice,
          formatAmount(draft.outstandingCents, locale, invoice.currency),
          draft.daysOverdue,
        ),
      );
    } catch (err) {
      setError(billingMessage(err, strings.billingReminderFailed));
    } finally {
      setReminding(null);
    }
  }

  return (
    <div className={styles.page}>
      <Toolbar label={strings.billingInvoices} className={styles.listBar}>
        <Input
          className="max-w-[380px] flex-1"
          type="search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={strings.billingSearchInvoices}
          aria-label={strings.billingSearchInvoices}
        />
        <ToolbarSpacer />
        <label className={styles.filterLabel}>
          {strings.billingFilterStatus}
          <Select
            value={filter}
            onChange={(e) => setFilter(e.target.value as Filter)}
          >
            {FILTERS.map((f) => (
              <option key={f.value} value={f.value}>
                {f.label()}
              </option>
            ))}
          </Select>
        </label>
        {loading && <Spinner size={16} />}
        {(invoices.length > 0 || filter !== "all") && (
          <Button onClick={() => void navigate("new")}>
            {strings.billingNewInvoice}
          </Button>
        )}
      </Toolbar>

      {error !== null && <ErrorBanner message={error} />}
      {reminded !== null && (
        <p className={styles.notice} role="status">
          {reminded}
        </p>
      )}

      {loading ? (
        <BillingLoading />
      ) : invoices.length === 0 && filter === "overdue" ? (
        <p className={styles.noMatches}>{strings.billingNothingOverdue}</p>
      ) : invoices.length === 0 && filter === "all" ? (
        <EmptyState
          Icon={FileText}
          title={strings.billingNoInvoicesTitle}
          body={strings.billingNoInvoicesBody}
          cta={strings.billingNewInvoice}
          onCta={() => void navigate("new")}
        />
      ) : shown.length === 0 ? (
        <p className={styles.noMatches}>{strings.billingNoMatches}</p>
      ) : (
        <><Table
          label={strings.billingInvoices}
          className={styles.listTable}
          stickyHeader
          interactiveRows
        >
          <thead>
            <tr>
              <Th>{strings.billingColNumber}</Th>
              <Th>{strings.billingColCustomer}</Th>
              <Th>{strings.billingColIssueDate}</Th>
              <Th>{strings.billingColDueDate}</Th>
              <Th>{strings.billingColStatus}</Th>
              <Th numeric>{strings.billingColTotal}</Th>
              <Th numeric>{strings.billingColOutstanding}</Th>
              <Th hideLabel>{strings.billingColActions}</Th>
            </tr>
          </thead>
          <tbody>
            {paged.records.map((invoice) => (
              <tr
                key={invoice.id}
                className={cx(invoice.overdue && styles.overdueRow)}
              >
                <td>
                  <button
                    type="button"
                    className={cx(styles.rowName, styles.mono)}
                    onClick={() => void navigate(invoice.id)}
                  >
                    {invoice.number ?? strings.billingNotNumbered}
                  </button>
                </td>
                <td>
                  {names.get(invoice.customerId) ??
                    strings.billingUnknownCustomer}
                </td>
                <td>
                  {formatDocumentDate(
                    invoice.issueDate,
                    locale,
                    strings.billingNoDate,
                  )}
                </td>
                <td>
                  {formatDocumentDate(
                    invoice.dueDate,
                    locale,
                    strings.billingNoDate,
                  )}
                </td>
                <td className={styles.chips}>
                  <DocumentChips invoice={invoice} />
                </td>
                <Td numeric>
                  {formatAmount(
                    invoice.totals.grossCents,
                    locale,
                    invoice.currency,
                  )}
                </Td>
                {/* The server's figure, like every other one here: what is
                    left after the payments recorded against this document. */}
                <Td numeric>
                  {formatAmount(
                    invoice.settlement.outstandingCents,
                    locale,
                    invoice.currency,
                  )}
                </Td>
                {/* Only a late document is chased. A draft owes nothing yet,
                    a settled one owes nothing any more, and the server
                    refuses both — so the button is not offered for them
                    rather than shown and then refused. */}
                <td className={styles.rowActions}>
                  {invoice.overdue && (
                    <button
                      type="button"
                      className={styles.linkAction}
                      disabled={reminding !== null}
                      title={strings.billingRemindHint}
                      onClick={() => void remind(invoice)}
                    >
                      {strings.billingRemind}
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </Table><BillingPagination {...paged} onPage={paged.setPage} /></>
      )}
    </div>
  );
}
