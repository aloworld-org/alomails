// Purchasing: every order we have placed, and the ones we are still waiting on
// (B5.09b).
//
// The list opens on **everything**, newest first, because the question a buyer
// arrives with is usually "where is that order" and not "show me one state".
// The filter is the server's own vocabulary and is sent to the server: a
// narrowing done here over a page the server had already truncated would be a
// filter that quietly lied.
//
// **Late is the server's word.** It computes it against its own today, so an
// order does not become late in one timezone an evening before another. The row
// wears it as a chip beside the state rather than as a colour alone.
//
// The only figure shown is the order's gross, exactly as the API sent it. There
// is no column that adds the page up: a page is not the sum of what is on order
// — the next page exists — and a total under a filtered list is the oldest way
// to make a screen lie.
import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Plus, Truck } from "lucide-react";

import { formatAmount } from "../billing";
import { Button, Spinner } from "../ds";
import { getLocale, strings } from "../i18n";
import { dayLabel, poStatusLabel, poStatusTone } from "./format";
import { inventoryMessage } from "./api";
import { useOrdersApi } from "./orders";
import { EmptyState, ErrorBanner, StatusChip } from "./parts";
import type { PurchaseOrderStatus, PurchaseOrderSummary } from "./types";
import styles from "./InventoryModule.module.css";

/** The states offered, in the order an order passes through them. */
const STATUSES: PurchaseOrderStatus[] = [
  "draft",
  "sent",
  "partially_received",
  "received",
  "cancelled",
];

export function PurchaseOrdersView() {
  const api = useOrdersApi();
  const navigate = useNavigate();
  const locale = getLocale();
  const [orders, setOrders] = useState<PurchaseOrderSummary[]>([]);
  const [status, setStatus] = useState<PurchaseOrderStatus | "">("");
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const read = await api.purchaseOrders(status === "" ? undefined : status);
        if (!live) return;
        setOrders(read);
        setError(null);
      } catch (err) {
        if (live) setError(inventoryMessage(err, strings.inventoryOrdersLoadFailed));
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [api, status]);

  const shown = useMemo(() => {
    const needle = search.trim().toLowerCase();
    if (needle === "") return orders;
    return orders.filter((order) =>
      `${order.number ?? ""} ${order.supplierName} ${order.reference}`
        .toLowerCase()
        .includes(needle),
    );
  }, [orders, search]);

  return (
    <div className={styles.page}>
      <div className={styles.toolbar}>
        <input
          className={styles.search}
          type="search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={strings.inventorySearchPurchaseOrders}
          aria-label={strings.inventorySearchPurchaseOrders}
        />
        <label className={styles.filterField}>
          {strings.inventoryFilterStatus}
          <select
            className={styles.select}
            value={status}
            onChange={(e) => setStatus(e.target.value as PurchaseOrderStatus | "")}
          >
            <option value="">{strings.inventoryAllStatuses}</option>
            {STATUSES.map((value) => (
              <option key={value} value={value}>
                {poStatusLabel(value)}
              </option>
            ))}
          </select>
        </label>
        <span className={styles.toolbarSpacer} />
        {loading && <Spinner size={16} />}
        <Button onClick={() => void navigate("/inventory/purchase-orders/new")}>
          <Plus size={16} /> {strings.inventoryNewPurchaseOrder}
        </Button>
      </div>

      {error !== null && <ErrorBanner message={error} />}

      {orders.length === 0 && !loading ? (
        <EmptyState
          Icon={Truck}
          title={
            status === ""
              ? strings.inventoryPurchaseOrdersEmptyTitle
              : strings.inventoryNoOrdersInState
          }
          body={strings.inventoryPurchaseOrdersEmptyBody}
          {...(status === ""
            ? {
                cta: strings.inventoryNewPurchaseOrder,
                onCta: () => void navigate("/inventory/purchase-orders/new"),
              }
            : {})}
        />
      ) : shown.length === 0 && !loading ? (
        <p className={styles.noMatches}>{strings.inventoryNoMatches}</p>
      ) : (
        <div className={styles.tableWrap}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th scope="col">{strings.inventoryColOrder}</th>
                <th scope="col">{strings.inventoryColSupplier}</th>
                <th scope="col">{strings.inventoryColExpected}</th>
                <th scope="col">{strings.inventoryColState}</th>
                <th scope="col" className={styles.numeric}>
                  {strings.inventoryColTotal}
                </th>
              </tr>
            </thead>
            <tbody>
              {shown.map((order) => (
                <tr key={order.id}>
                  <td>
                    <button
                      type="button"
                      className={styles.rowName}
                      onClick={() => void navigate(`/inventory/purchase-orders/${order.id}`)}
                    >
                      {order.number ?? strings.inventoryDraftOrder}
                    </button>
                    {order.reference !== "" && (
                      <span className={styles.subtle}>{order.reference}</span>
                    )}
                  </td>
                  <td>{order.supplierName}</td>
                  <td className={styles.muted}>{dayLabel(order.expectedDate)}</td>
                  <td>
                    <span className={styles.chips}>
                      <StatusChip
                        tone={poStatusTone(order.status)}
                        label={poStatusLabel(order.status)}
                      />
                      {order.late && (
                        <StatusChip tone="warn" label={strings.inventoryOrderLate} />
                      )}
                    </span>
                  </td>
                  <td className={styles.numeric}>
                    {formatAmount(order.totals.grossCents, locale, order.currency)}
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
