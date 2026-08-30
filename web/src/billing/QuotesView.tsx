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
import { Eye, FileSignature, MoreHorizontal, Trash2 } from "lucide-react";

import {
  Button,
  Input,
  Menu,
  Select,
  Spinner,
  Table,
  Td,
  Th,
  Toolbar,
  ToolbarSpacer,
  useDialogs,
  cx,
} from "../ds";
import { strings, useLocale } from "../i18n";
import { billingMessage, useBillingApi } from "./api";
import { BillingPagination } from "./BillingPagination";
import { BillingStatusCell } from "./BillingStatusCell";
import {
  formatAuditDate,
  formatAuditDateTime,
  formatDocumentDate,
} from "./dates";
import { formatAmount } from "./money";
import { BillingLoading, EmptyState, ErrorBanner } from "./parts";
import { QuoteChips } from "./status";
import type {
  BillingCustomer,
  BillingQuoteSummary,
  QuoteStatus,
} from "./types";
import styles from "./billingStyles";
import { useBillingPagination } from "./useBillingPagination";

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
function matches(
  quote: BillingQuoteSummary,
  customer: string,
  needle: string,
): boolean {
  if (needle === "") return true;
  return [quote.number ?? "", customer, quote.reference]
    .join(" ")
    .toLowerCase()
    .includes(needle);
}

export function QuotesView() {
  const api = useBillingApi();
  const locale = useLocale();
  const navigate = useNavigate();
  const { confirm } = useDialogs();
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
        api.quotes(
          filter === "all" ? undefined : (filter satisfies QuoteStatus),
        ),
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

  const names = useMemo(
    () => new Map(customers.map((c) => [c.id, c.name] as const)),
    [customers],
  );

  const shown = useMemo(() => {
    const needle = search.trim().toLowerCase();
    const matchesSearch = quotes.filter((q) =>
      matches(q, names.get(q.customerId) ?? "", needle),
    );
    if (filter !== "draft") return matchesSearch;
    return [...matchesSearch].sort(
      (a, b) =>
        new Date(b.updatedAt).getTime() - new Date(a.updatedAt).getTime(),
    );
  }, [quotes, names, search, filter]);
  const paged = useBillingPagination(shown, `${search}\u0000${filter}`);

  const deleteDraft = useCallback(
    async (quote: BillingQuoteSummary) => {
      if (quote.status !== "draft") return;
      const accepted = await confirm({
        title: strings.billingDeleteQuoteDraft,
        message: strings.billingDeleteQuoteDraftConfirm,
        confirmLabel: strings.billingDeleteQuoteDraft,
        danger: true,
      });
      if (!accepted) return;
      try {
        await api.deleteQuote(quote.id);
        setQuotes((current) => current.filter((item) => item.id !== quote.id));
        setError(null);
      } catch (err) {
        setError(billingMessage(err, strings.billingActionFailed));
      }
    },
    [api, confirm],
  );

  return (
    <div className={cx(styles.page, "!pb-6 max-[52rem]:!pb-4")}>
      <Toolbar label={strings.billingQuotes} className={styles.listBar}>
        <Input
          className="max-w-[380px] flex-1"
          type="search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={strings.billingSearchQuotes}
          aria-label={strings.billingSearchQuotes}
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
        {(quotes.length > 0 || filter !== "all") && (
          <Button onClick={() => void navigate("new")}>
            {strings.billingNewQuote}
          </Button>
        )}
      </Toolbar>

      {error !== null && (
        <ErrorBanner
          message={error}
          presentation="popup"
          onDismiss={() => setError(null)}
        />
      )}

      {loading ? (
        <BillingLoading />
      ) : quotes.length === 0 && filter === "all" ? (
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
        <><Table
          label={strings.billingQuotes}
          className={styles.listTable}
          stickyHeader
          interactiveRows
        >
          <thead>
            <tr>
              <Th>{strings.billingColNumber}</Th>
              <Th>{strings.billingColCustomer}</Th>
              <Th>{strings.billingColCreated}</Th>
              <Th>{strings.billingColLastEdited}</Th>
              <Th>{strings.billingColSentDate}</Th>
              <Th>{strings.billingColValidUntil}</Th>
              <Th>{strings.billingColStatus}</Th>
              <Th numeric>{strings.billingColTotal}</Th>
              <Th className="w-14">
                <span className="sr-only">{strings.billingColActions}</span>
              </Th>
            </tr>
          </thead>
          <tbody>
            {paged.records.map((quote) => (
              <tr
                key={quote.id}
                role="link"
                tabIndex={0}
                aria-label={`${quote.number ?? strings.billingDraftQuote}: ${
                  names.get(quote.customerId) ?? strings.billingUnknownCustomer
                }`}
                className="group cursor-pointer focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-[-2px]"
                onClick={() => void navigate(quote.id)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    void navigate(quote.id);
                  }
                }}
              >
                <td>
                  <span className={cx(styles.rowName, styles.mono)}>
                    {quote.number ?? strings.billingDraftQuote}
                  </span>
                </td>
                <td>
                  {names.get(quote.customerId) ??
                    strings.billingUnknownCustomer}
                </td>
                <td>
                  <time
                    dateTime={quote.createdAt}
                    title={formatAuditDateTime(quote.createdAt, locale)}
                  >
                    {formatAuditDate(quote.createdAt, locale)}
                  </time>
                </td>
                <td>
                  <time
                    dateTime={quote.updatedAt}
                    title={formatAuditDateTime(quote.updatedAt, locale)}
                  >
                    {formatAuditDate(quote.updatedAt, locale)}
                  </time>
                </td>
                <td>
                  {formatDocumentDate(
                    quote.sentDate,
                    locale,
                    strings.billingNoDate,
                  )}
                </td>
                <td>
                  {formatDocumentDate(
                    quote.validUntil,
                    locale,
                    strings.billingNoDate,
                  )}
                </td>
                <BillingStatusCell>
                  <QuoteChips quote={quote} />
                </BillingStatusCell>
                <Td numeric>
                  {formatAmount(
                    quote.totals.grossCents,
                    locale,
                    quote.currency,
                  )}
                </Td>
                <td
                  className="w-14 px-2 text-right"
                  onClick={(event) => event.stopPropagation()}
                  onKeyDown={(event) => event.stopPropagation()}
                >
                  <span className="inline-flex opacity-60 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100">
                    <Menu
                      label={strings.moreActions}
                      icon={<MoreHorizontal size={18} />}
                      items={[
                        {
                          key: "preview",
                          label: strings.billingQuotationPreview,
                          icon: <Eye />,
                          onClick: () => void navigate(`${quote.id}?preview=1`),
                        },
                        ...(quote.status === "draft"
                          ? [
                              {
                                key: "delete",
                                label: strings.billingDeleteQuoteDraft,
                                icon: <Trash2 />,
                                danger: true,
                                divider: true,
                                onClick: () => void deleteDraft(quote),
                              },
                            ]
                          : []),
                      ]}
                    />
                  </span>
                </td>
              </tr>
            ))}
          </tbody>
        </Table><BillingPagination {...paged} onPage={paged.setPage} /></>
      )}
    </div>
  );
}
