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
import {
  Button,
  ChoicePicker,
  Input,
  Spinner,
  Table,
  Td,
  Th,
  Toolbar,
  ToolbarSpacer,
} from "../ds";
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
    <div className={`${styles.page} ${styles.listPage}`}>
      <section className={styles.listWorkspace}>
        <div className={styles.pageHeading}>
          <div className={styles.pageHeadingCopy}>
            <h2 className={styles.pageTitle}>{strings.inventoryTabSales}</h2>
            <p className={styles.pageSubtitle}>{strings.inventorySalesPurpose}</p>
          </div>
          {orders.length > 0 && (
            <Button icon={<Plus size={16} />} onClick={() => void navigate("/inventory/sales-orders/new")}>
              {strings.inventoryNewSalesOrder}
            </Button>
          )}
        </div>
      <Toolbar label={strings.inventoryTabSales} surface="plain" className={styles.listFilters}>
        <Input
          className="basis-[260px] max-[48rem]:basis-full"
          type="search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={strings.inventorySearchSalesOrders}
          aria-label={strings.inventorySearchSalesOrders}
        />
        <div className={styles.statusFilter}>
          <span>{strings.inventoryFilterStatus}</span>
          <ChoicePicker
            value={status}
            label={strings.inventoryFilterStatus}
            placeholder={strings.inventoryAllStatuses}
            options={[{ value: "", label: strings.inventoryAllStatuses }, ...STATUSES.map((value) => ({ value, label: soStatusLabel(value) }))]}
            onChange={(value) => setStatus(value as SalesOrderStatus | "")}
          />
        </div>
        <ToolbarSpacer />
        {loading && <Spinner size={16} />}
      </Toolbar>
      </section>

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
        <Table label={strings.inventoryTabSales} interactiveRows>
          <thead>
            <tr>
              <Th>{strings.inventoryColOrder}</Th>
              <Th>{strings.inventoryColCustomer}</Th>
              <Th>{strings.inventoryColPromised}</Th>
              <Th>{strings.inventoryColState}</Th>
              <Th numeric>{strings.inventoryColTotal}</Th>
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
                  <span className="inline-flex flex-wrap items-center gap-2">
                    <StatusChip
                      tone={soStatusTone(order.status)}
                      label={soStatusLabel(order.status)}
                    />
                    {order.late && <StatusChip tone="warn" label={strings.inventoryOrderLate} />}
                  </span>
                </td>
                <Td numeric>
                  {formatAmount(order.totals.grossCents, locale, order.currency)}
                </Td>
              </tr>
            ))}
          </tbody>
        </Table>
      )}
    </div>
  );
}
