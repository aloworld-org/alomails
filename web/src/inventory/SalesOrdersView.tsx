// Sales orders: what customers have asked us for, and what is still to go out
// (B5.09b).
//
// The mirror of the purchasing list, and deliberately the same screen: same
// filter over the server's own vocabulary, same server-computed `late`, same
// single money column carrying the API's gross and nothing this page added up.
// A person who has learned one of the two lists has learned both — the only
// thing that changes is which way the goods are going.
import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { ClipboardList, Plus } from "lucide-react";

import { formatAmount } from "../billing";
import { Button, Spinner } from "../ds";
import { getLocale, strings } from "../i18n";
import { dayLabel, soStatusLabel, soStatusTone } from "./format";
import { inventoryMessage } from "./api";
import { useOrdersApi } from "./orders";
import { EmptyState, ErrorBanner, StatusChip } from "./parts";
import type { SalesOrderStatus, SalesOrderSummary } from "./types";
import styles from "./InventoryModule.module.css";

/** The states offered, in the order an order passes through them. */
const STATUSES: SalesOrderStatus[] = [
  "draft",
  "confirmed",
  "partially_delivered",
  "delivered",
  "cancelled",
];

export function SalesOrdersView() {
  const api = useOrdersApi();
  const navigate = useNavigate();
  const locale = getLocale();
  const [orders, setOrders] = useState<SalesOrderSummary[]>([]);
  const [status, setStatus] = useState<SalesOrderStatus | "">("");
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const read = await api.salesOrders(status === "" ? undefined : status);
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
      `${order.number ?? ""} ${order.customerName} ${order.reference}`
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
          placeholder={strings.inventorySearchSalesOrders}
          aria-label={strings.inventorySearchSalesOrders}
        />
        <label className={styles.filterField}>
          {strings.inventoryFilterStatus}
          <select
            className={styles.select}
            value={status}
            onChange={(e) => setStatus(e.target.value as SalesOrderStatus | "")}
          >
            <option value="">{strings.inventoryAllStatuses}</option>
            {STATUSES.map((value) => (
              <option key={value} value={value}>
                {soStatusLabel(value)}
              </option>
            ))}
          </select>
        </label>
        <span className={styles.toolbarSpacer} />
        {loading && <Spinner size={16} />}
        <Button onClick={() => void navigate("/inventory/sales-orders/new")}>
          <Plus size={16} /> {strings.inventoryNewSalesOrder}
        </Button>
      </div>

      {error !== null && <ErrorBanner message={error} />}

      {orders.length === 0 && !loading ? (
        <EmptyState
          Icon={ClipboardList}
          title={
            status === "" ? strings.inventorySalesOrdersEmptyTitle : strings.inventoryNoOrdersInState
          }
          body={strings.inventorySalesOrdersEmptyBody}
          {...(status === ""
            ? {
                cta: strings.inventoryNewSalesOrder,
                onCta: () => void navigate("/inventory/sales-orders/new"),
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
                <th scope="col">{strings.inventoryColCustomer}</th>
                <th scope="col">{strings.inventoryColPromised}</th>
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
                      onClick={() => void navigate(`/inventory/sales-orders/${order.id}`)}
                    >
                      {order.number ?? strings.inventoryDraftOrder}
                    </button>
                    {order.reference !== "" && (
                      <span className={styles.subtle}>{order.reference}</span>
                    )}
                  </td>
                  <td>{order.customerName}</td>
                  <td className={styles.muted}>{dayLabel(order.expectedDate)}</td>
                  <td>
                    <span className={styles.chips}>
                      <StatusChip
                        tone={soStatusTone(order.status)}
                        label={soStatusLabel(order.status)}
                      />
                      {order.late && <StatusChip tone="warn" label={strings.inventoryOrderLate} />}
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
