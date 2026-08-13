// What visitors asked to buy, in one screen (ADR 0036 / ADR 0041, S2.12c2):
// the queue on the left and the order being answered beside it, like the
// contact inbox next door, so a person who has answered an enquiry already
// knows how this works.
//
// Three properties this screen must keep, because each one is somebody's
// Saturday morning:
//
//   * **Nothing here is a second copy.** The lines, the prices and the total
//     are the order's own, frozen from the publish the visitor ordered from —
//     the screen formats minor units with the exponent the server sent and
//     computes nothing.
//   * **The workflow moves both ways.** An order cancelled by mistake is
//     confirmed again in one click rather than re-typed; the status buttons
//     are a state, not a one-way ratchet.
//   * **Deleting really deletes.** An order carries a member of the public's
//     name, address and phone number, so the row a customer asks to be
//     removed is removed — which is why it asks once before it does it.
import { useCallback, useEffect, useMemo, useState } from "react";
import { ArrowLeft, Download, ShoppingBag, Trash2 } from "lucide-react";
import { Link, useNavigate, useParams } from "react-router-dom";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { saveTextFile } from "../platform/download";
import { sitesMessage, useSitesApi } from "./api";
import { formatPrice } from "./catalogPricing";
import { EmptyState, ErrorBanner } from "./parts";
import type { SiteDetail, SiteOrder, SiteOrderStatus } from "./types";
import styles from "./SitesModule.module.css";

const received = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "short",
});

/** The four workflow words, left to right as an order travels through them. */
const STATUSES: readonly SiteOrderStatus[] = ["new", "confirmed", "fulfilled", "cancelled"];

/** What to call a status in the reader's language. The server owns the word
 *  itself; this is the only place it is translated. */
export function orderStatusLabel(status: SiteOrderStatus): string {
  switch (status) {
    case "new":
      return strings.sitesOrderStatusNew;
    case "confirmed":
      return strings.sitesOrderStatusConfirmed;
    case "fulfilled":
      return strings.sitesOrderStatusFulfilled;
    case "cancelled":
      return strings.sitesOrderStatusCancelled;
  }
}

/** A line's own money, or the honest blank of an item quoted by hand — which
 *  is not a price of zero and must never be shown as one. */
function lineMoney(cents: number | null, order: SiteOrder): string {
  return cents === null
    ? strings.sitesOrderLineNoPrice
    : formatPrice(cents, order.currency, order.currencyExponent);
}

export function OrdersView() {
  const { siteId = "" } = useParams();
  const navigate = useNavigate();
  const api = useSitesApi();
  const [site, setSite] = useState<SiteDetail | null>(null);
  const [orders, setOrders] = useState<SiteOrder[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [filter, setFilter] = useState<SiteOrderStatus | "all">("all");
  const [loading, setLoading] = useState(true);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [armedId, setArmedId] = useState<string | null>(null);
  const [exporting, setExporting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [detail, rows] = await Promise.all([api.site(siteId), api.orders(siteId)]);
      setSite(detail);
      setOrders(rows);
      setSelectedId((current) =>
        current !== null && rows.some((row) => row.id === current)
          ? current
          : (rows[0]?.id ?? null),
      );
      setError(null);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesOrdersLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, siteId]);

  useEffect(() => {
    void load();
  }, [load]);

  const shown = useMemo(
    () => (filter === "all" ? orders : orders.filter((order) => order.status === filter)),
    [filter, orders],
  );
  const selected = useMemo(
    () => orders.find((order) => order.id === selectedId) ?? null,
    [orders, selectedId],
  );

  /** How many orders each filter would show — a count nobody should have to
   *  get by clicking through the tabs. */
  const counts = useMemo(() => {
    const tally = new Map<SiteOrderStatus, number>();
    for (const order of orders) {
      tally.set(order.status, (tally.get(order.status) ?? 0) + 1);
    }
    return tally;
  }, [orders]);

  async function move(order: SiteOrder, status: SiteOrderStatus) {
    if (order.status === status) return;
    setBusyId(order.id);
    setError(null);
    try {
      const stored = await api.setOrderStatus(siteId, order.id, status);
      setOrders((rows) => rows.map((row) => (row.id === stored.id ? stored : row)));
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesOrderStatusFailed));
    } finally {
      setBusyId(null);
    }
  }

  async function remove(order: SiteOrder) {
    if (armedId !== order.id) {
      setArmedId(order.id);
      return;
    }
    setBusyId(order.id);
    setError(null);
    try {
      await api.deleteOrder(siteId, order.id);
      setOrders((rows) => {
        const left = rows.filter((row) => row.id !== order.id);
        setSelectedId(left[0]?.id ?? null);
        return left;
      });
      setArmedId(null);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesOrderDeleteFailed));
    } finally {
      setBusyId(null);
    }
  }

  async function exportCsv() {
    if (site === null) return;
    setExporting(true);
    setError(null);
    try {
      const csv = await api.ordersCsv(siteId);
      saveTextFile(csv, `orders-${site.subdomain}.csv`, "text/csv;charset=utf-8");
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesOrdersExportFailed));
    } finally {
      setExporting(false);
    }
  }

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <Link to=".." relative="path" className={styles.backLink}>
          <ArrowLeft size="var(--icon-size-inline)" aria-hidden="true" />
          {strings.sitesBack}
        </Link>
        <div className={styles.siteHead}>
          <h1 className={styles.title}>{strings.sitesOrders}</h1>
          {site !== null && <span className={styles.submissionSiteName}>{site.name}</span>}
        </div>
        <div className={styles.headerActions}>
          {loading && <Spinner size={16} />}
          <Button
            variant="secondary"
            size="sm"
            icon={<Download size="var(--icon-size-inline)" />}
            disabled={site === null || orders.length === 0 || exporting}
            onClick={() => void exportCsv()}
          >
            {exporting ? strings.sitesOrdersExporting : strings.sitesOrdersExport}
          </Button>
        </div>
      </header>

      {error !== null && <ErrorBanner message={error} />}

      {!loading && orders.length === 0 ? (
        <EmptyState
          Icon={ShoppingBag}
          title={strings.sitesNoOrdersTitle}
          body={strings.sitesNoOrdersBody}
          cta={strings.sitesCatalogs}
          onCta={() => navigate(`/sites/${encodeURIComponent(siteId)}/catalogs`)}
        />
      ) : (
        <>
          <div className={styles.orderFilters} role="group" aria-label={strings.sitesOrderFilter}>
            {(["all", ...STATUSES] as Array<SiteOrderStatus | "all">).map((value) => {
              const label =
                value === "all" ? strings.sitesOrderFilterAll : orderStatusLabel(value);
              const count = value === "all" ? orders.length : (counts.get(value) ?? 0);
              return (
                <button
                  key={value}
                  type="button"
                  className={`${styles.orderFilter} ${
                    filter === value ? styles.orderFilterActive : ""
                  }`}
                  aria-pressed={filter === value}
                  onClick={() => setFilter(value)}
                >
                  {strings.sitesOrderFilterOption(label, count)}
                </button>
              );
            })}
          </div>

          <div className={styles.submissionsLayout}>
            <section className={styles.submissionList} aria-label={strings.sitesOrderList}>
              {shown.length === 0 ? (
                <p className={styles.orderFilterEmpty}>{strings.sitesOrderFilterEmpty}</p>
              ) : (
                shown.map((order) => (
                  <button
                    type="button"
                    key={order.id}
                    className={`${styles.submissionRow} ${
                      selectedId === order.id ? styles.submissionRowSelected : ""
                    }`}
                    onClick={() => setSelectedId(order.id)}
                    aria-pressed={selectedId === order.id}
                  >
                    <span className={styles.submissionRowTop}>
                      <strong>{order.customerName}</strong>
                      <time dateTime={order.receivedAt}>
                        {received.format(new Date(order.receivedAt))}
                      </time>
                    </span>
                    <span className={styles.submissionEmail}>
                      {strings.sitesOrderLineCount(order.lines.length)} ·{" "}
                      {formatPrice(order.totalCents, order.currency, order.currencyExponent)}
                    </span>
                    <span className={styles.submissionRowBottom}>
                      <span>{order.catalogName}</span>
                      <span
                        className={order.status === "new" ? styles.open : styles.handled}
                      >
                        {orderStatusLabel(order.status)}
                      </span>
                    </span>
                  </button>
                ))
              )}
            </section>

            {selected !== null && (
              <article className={styles.submissionDetail} aria-label={strings.sitesOrderDetail}>
                <header className={styles.submissionDetailHead}>
                  <div>
                    <h2>{selected.customerName}</h2>
                    <a href={`mailto:${selected.customerEmail}`}>{selected.customerEmail}</a>
                  </div>
                  <Button
                    variant={armedId === selected.id ? "danger" : "ghost"}
                    size="sm"
                    icon={<Trash2 size="var(--icon-size-inline)" />}
                    disabled={busyId === selected.id}
                    onClick={() => void remove(selected)}
                  >
                    {armedId === selected.id
                      ? strings.sitesOrderDeleteConfirm
                      : strings.sitesOrderDelete}
                  </Button>
                </header>

                {armedId === selected.id && (
                  <p className={styles.hint}>{strings.sitesOrderDeleteHint}</p>
                )}

                <dl className={styles.submissionMeta}>
                  <div>
                    <dt>{strings.sitesOrderCatalog}</dt>
                    <dd>{selected.catalogName}</dd>
                  </div>
                  <div>
                    <dt>{strings.sitesReceived}</dt>
                    <dd>{received.format(new Date(selected.receivedAt))}</dd>
                  </div>
                  {selected.customerPhone !== null && (
                    <div>
                      <dt>{strings.sitesOrderPhone}</dt>
                      <dd>
                        <a href={`tel:${selected.customerPhone}`}>{selected.customerPhone}</a>
                      </dd>
                    </div>
                  )}
                </dl>

                <div className={styles.orderStatusBar} role="group" aria-label={strings.sitesOrderStatus}>
                  {STATUSES.map((status) => (
                    <Button
                      key={status}
                      variant={selected.status === status ? "primary" : "ghost"}
                      size="sm"
                      aria-pressed={selected.status === status}
                      disabled={busyId === selected.id}
                      onClick={() => void move(selected, status)}
                    >
                      {orderStatusLabel(status)}
                    </Button>
                  ))}
                </div>

                <table className={styles.orderLines}>
                  <caption className={styles.orderLinesCaption}>
                    {strings.sitesOrderLinesCaption}
                  </caption>
                  <thead>
                    <tr>
                      <th scope="col">{strings.sitesOrderItem}</th>
                      <th scope="col">{strings.sitesOrderQuantity}</th>
                      <th scope="col">{strings.sitesOrderUnitPrice}</th>
                      <th scope="col">{strings.sitesOrderLineTotal}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {selected.lines.map((line) => (
                      <tr key={line.itemSlug}>
                        <th scope="row">{line.itemName}</th>
                        <td>{line.quantity}</td>
                        <td>{lineMoney(line.unitPriceCents, selected)}</td>
                        <td>{lineMoney(line.lineTotalCents, selected)}</td>
                      </tr>
                    ))}
                  </tbody>
                  <tfoot>
                    <tr>
                      <th scope="row" colSpan={3}>
                        {strings.sitesOrderTotal}
                      </th>
                      <td>
                        {formatPrice(
                          selected.totalCents,
                          selected.currency,
                          selected.currencyExponent,
                        )}
                      </td>
                    </tr>
                  </tfoot>
                </table>
                {selected.lines.some((line) => line.lineTotalCents === null) && (
                  <p className={styles.hint}>{strings.sitesOrderQuotedHint}</p>
                )}

                {selected.note !== null && (
                  <p className={styles.submissionMessage}>{selected.note}</p>
                )}
              </article>
            )}
          </div>
        </>
      )}
    </div>
  );
}
