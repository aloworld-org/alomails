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
    <div className={`${styles.page} ${styles.listPage}`}>
      <section className={styles.listWorkspace}>
        <div className={styles.pageHeading}>
          <div className={styles.pageHeadingCopy}>
            <h2 className={styles.pageTitle}>{strings.inventoryTabPurchasing}</h2>
            <p className={styles.pageSubtitle}>{strings.inventoryPurchasingPurpose}</p>
          </div>
          {orders.length > 0 && (
            <Button icon={<Plus size={16} />} onClick={() => void navigate("/inventory/purchase-orders/new")}>
              {strings.inventoryNewPurchaseOrder}
            </Button>
          )}
        </div>
      <Toolbar label={strings.inventoryTabPurchasing} surface="plain" className={styles.listFilters}>
        <Input
          className="basis-[260px] max-[48rem]:basis-full"
          type="search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={strings.inventorySearchPurchaseOrders}
          aria-label={strings.inventorySearchPurchaseOrders}
        />
        <div className={styles.statusFilter}>
          <span>{strings.inventoryFilterStatus}</span>
          <ChoicePicker
            value={status}
            label={strings.inventoryFilterStatus}
            placeholder={strings.inventoryAllStatuses}
            options={[{ value: "", label: strings.inventoryAllStatuses }, ...STATUSES.map((value) => ({ value, label: poStatusLabel(value) }))]}
            onChange={(value) => setStatus(value as PurchaseOrderStatus | "")}
          />
        </div>
        <ToolbarSpacer />
        {loading && <Spinner size={16} />}
      </Toolbar>
      </section>

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
        <Table label={strings.inventoryTabPurchasing} interactiveRows>
          <thead>
            <tr>
              <Th>{strings.inventoryColOrder}</Th>
              <Th>{strings.inventoryColSupplier}</Th>
              <Th>{strings.inventoryColExpected}</Th>
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
                  <span className="inline-flex flex-wrap items-center gap-2">
                    <StatusChip
                      tone={poStatusTone(order.status)}
                      label={poStatusLabel(order.status)}
                    />
                    {order.late && (
                      <StatusChip tone="warn" label={strings.inventoryOrderLate} />
                    )}
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
