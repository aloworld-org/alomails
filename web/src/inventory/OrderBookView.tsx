// The order book (alo Orders, item O1.d) — the one screen that answers what a
// manufacturer opens in the morning: what have we promised, what is set aside
// for it, what has gone out, what have we billed, and what are we still owed.
//
// **Every number on this page came off the wire.** There is no arithmetic in
// this file, not even the subtractions that look safe: an order that was
// short-closed is owed nothing, while ordered-minus-delivered still says it is
// owed. The store knows which orders were given up on and this screen does not,
// so it asks and prints the answer. A screen that added up its own rows would
// eventually disagree with the server about what a business is owed, and a
// reader would have no way to tell which of the two was right.
//
// The totals row is withheld when the book spans two currencies. Adding euros
// to pounds produces a figure that looks authoritative and means nothing; the
// honest screen says so and leaves each order's own figures, which are exact.
//
// It is a **read**, so it has no actions of its own. Every row is a way into
// the order it stands for, where the acts live.
import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { BookOpen } from "lucide-react";

import { formatAmount } from "../billing";
import { Spinner } from "../ds";
import { getLocale, strings } from "../i18n";
import { qtyLabel, soStatusLabel, soStatusTone } from "./format";
import { inventoryMessage } from "./api";
import { useOrdersApi } from "./orders";
import { EmptyState, ErrorBanner, StatusChip } from "./parts";
import type { BookScope, OrderBook } from "./types";
import styles from "./InventoryModule.module.css";

/** The two scopes, in the order they are offered: the question first, the
 *  archive second. */
const SCOPES: { value: BookScope; label: () => string }[] = [
  { value: "open", label: () => strings.inventoryScopeOpen },
  { value: "all", label: () => strings.inventoryScopeAll },
];

/** "EUR and GBP", "EUR et GBP", "EUR en GBP" — the joining word is the
 *  language's, not a comma we chose for it. */
function currencyList(currencies: string[], locale: string): string {
  try {
    return new Intl.ListFormat(locale, { style: "long", type: "conjunction" }).format(currencies);
  } catch {
    // An engine without `ListFormat`, or a locale tag it will not take. The
    // codes are the part that matters; the conjunction is not worth an error.
    return currencies.join(", ");
  }
}

export function OrderBookView() {
  const api = useOrdersApi();
  const navigate = useNavigate();
  const locale = getLocale();
  const [book, setBook] = useState<OrderBook | null>(null);
  const [scope, setScope] = useState<BookScope>("open");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const read = await api.orderBook(scope);
        if (!live) return;
        setBook(read);
        setError(null);
      } catch (err) {
        // The stale book stays on screen behind the banner. A reader who was
        // looking at a figure when the network dropped should not have it
        // replaced by nothing.
        if (live) setError(inventoryMessage(err, strings.inventoryOrderBookLoadFailed));
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [api, scope]);

  const orders = book?.orders ?? [];
  // One currency means the totals are a real sum; more than one means they are
  // not, and the screen says that instead of printing them.
  const currencies = book?.currencies ?? [];
  const totalCurrency = currencies.length === 1 ? currencies[0] : undefined;

  return (
    <div className={styles.page}>
      <div className={styles.toolbar}>
        <label className={styles.filterField}>
          {strings.inventoryFilterScope}
          <select
            className={styles.select}
            value={scope}
            onChange={(e) => setScope(e.target.value as BookScope)}
          >
            {SCOPES.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label()}
              </option>
            ))}
          </select>
        </label>
        <span className={styles.toolbarSpacer} />
        {loading && <Spinner size={16} />}
      </div>

      {error !== null && <ErrorBanner message={error} />}

      {orders.length === 0 && !loading ? (
        <EmptyState
          Icon={BookOpen}
          title={
            scope === "open"
              ? strings.inventoryOrderBookEmptyTitle
              : strings.inventoryOrderBookEmptyAllTitle
          }
          body={
            scope === "open"
              ? strings.inventoryOrderBookEmptyBody
              : strings.inventoryOrderBookEmptyAllBody
          }
        />
      ) : (
        <div className={styles.tableWrap}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th scope="col">{strings.inventoryColOrder}</th>
                <th scope="col">{strings.inventoryColCustomer}</th>
                <th scope="col">{strings.inventoryColState}</th>
                <th scope="col" className={styles.numeric}>
                  {strings.inventoryColOrdered}
                </th>
                <th scope="col" className={styles.numeric}>
                  {strings.inventoryColReserved}
                </th>
                <th scope="col" className={styles.numeric}>
                  {strings.inventoryColDelivered}
                </th>
                <th scope="col" className={styles.numeric}>
                  {strings.inventoryColInvoiced}
                </th>
                <th scope="col" className={styles.numeric}>
                  {strings.inventoryColOutstanding}
                </th>
              </tr>
            </thead>
            <tbody>
              {orders.map((row) => (
                <tr key={row.id}>
                  <td>
                    <button
                      type="button"
                      className={styles.rowName}
                      onClick={() => void navigate(`/inventory/sales-orders/${row.id}`)}
                    >
                      {row.number ?? strings.inventoryDraftOrder}
                    </button>
                  </td>
                  <td>{row.customerName}</td>
                  <td>
                    <StatusChip
                      tone={soStatusTone(row.status)}
                      label={soStatusLabel(row.status)}
                    />
                  </td>
                  <td className={styles.numeric}>
                    {formatAmount(row.figures.orderedNetCents, locale, row.currency)}
                  </td>
                  <td className={styles.numeric}>
                    {formatAmount(row.figures.reservedNetCents, locale, row.currency)}
                  </td>
                  <td className={styles.numeric}>
                    {formatAmount(row.figures.deliveredNetCents, locale, row.currency)}
                  </td>
                  <td className={styles.numeric}>
                    {formatAmount(row.figures.invoicedNetCents, locale, row.currency)}
                  </td>
                  <td className={styles.numeric}>
                    {formatAmount(row.figures.outstandingNetCents, locale, row.currency)}
                    {/* What is owed in goods, beside what it is owed in money —
                        zero on an order of pure services, which is why it is
                        shown only when there is something to move. */}
                    {row.figures.outstandingQtyMilli !== 0 && (
                      <span className={styles.subtle}>
                        {strings.inventoryBookQtyHint(qtyLabel(row.figures.outstandingQtyMilli))}
                      </span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
            {book !== null && (
              <tfoot>
                {totalCurrency === undefined ? (
                  <tr>
                    <td colSpan={8} className={styles.muted}>
                      {strings.inventoryBookMixedCurrencies(currencyList(currencies, locale))}
                    </td>
                  </tr>
                ) : (
                  <tr>
                    <th scope="row" colSpan={3}>
                      {strings.inventoryBookTotal}
                    </th>
                    <td className={styles.numeric}>
                      {formatAmount(book.totals.orderedNetCents, locale, totalCurrency)}
                    </td>
                    <td className={styles.numeric}>
                      {formatAmount(book.totals.reservedNetCents, locale, totalCurrency)}
                    </td>
                    <td className={styles.numeric}>
                      {formatAmount(book.totals.deliveredNetCents, locale, totalCurrency)}
                    </td>
                    <td className={styles.numeric}>
                      {formatAmount(book.totals.invoicedNetCents, locale, totalCurrency)}
                    </td>
                    <td className={styles.numeric}>
                      {formatAmount(book.totals.outstandingNetCents, locale, totalCurrency)}
                    </td>
                  </tr>
                )}
              </tfoot>
            )}
          </table>
        </div>
      )}
    </div>
  );
}
