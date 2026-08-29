// One sales order, from the draft nobody has answered to the invoice for what
// went out (B5.09b).
//
// The mirror of the purchasing screen, with the two differences that matter:
//
//  - **Confirming writes no letter.** It draws the number and freezes the
//    document, and that is all: confirming records an answer the customer
//    already has, and telling them again is an ordinary message through the one
//    audited submission path.
//  - **Billing is its own act, and it bills what has *gone out*.** The button
//    raises a **draft** invoice for the delivered-and-not-yet-billed quantity —
//    the store's own figure, shown on each line as "to bill" so the number a
//    person sees is the number the button uses. It consumes nothing from the
//    gapless series; issuing it is a decision somebody makes in Billing.
//
// Everything else — the line grid, the consignment sheet, the frozen document —
// is the same code as purchasing's, because it is the same document pointed the
// other way.
import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { ArrowLeft } from "lucide-react";

import { RecordHistory } from "../audit";
import { TotalsPanel, useBillingApi, useCustomers, type BillingProduct } from "../billing";
import {
  Button,
  Field,
  Input,
  Select,
  Spinner,
  Toolbar,
  ToolbarSpacer,
  useDialogs,
} from "../ds";
import { strings } from "../i18n";
import { inventoryMessage, useInventoryApi } from "./api";
import { dayLabel, qtyLabel, soStatusLabel, soStatusTone } from "./format";
import { FulfilDialog } from "./FulfilDialog";
import { FulfilmentList, type FulfilmentEntry } from "./Fulfilments";
import { OrderLines } from "./OrderLines";
import { blankOrderRow, orderRowFromLine, orderRowsDraft, type OrderRow } from "./orderRows";
import { useOrdersApi } from "./orders";
import { ErrorBanner, StatusChip } from "./parts";
import type {
  Delivery,
  InvLocation,
  OrderPatch,
  SalesOrder,
  SalesOrderInvoice,
} from "./types";
import styles from "./InventoryModule.module.css";

/** Whether the document can still be edited. */
function isDraft(order: SalesOrder | null): boolean {
  return order === null || order.status === "draft";
}

/** Whether goods can still go out against it. */
function isOpen(order: SalesOrder): boolean {
  return order.status === "confirmed" || order.status === "partially_delivered";
}

/** Whether anything on it can still be billed — the server's own per-line
 *  figure, summed only to answer "is there anything at all", never to state a
 *  quantity to a person. */
function hasBillable(order: SalesOrder): boolean {
  return order.lines.some((line) => line.invoiceableQtyMilli > 0);
}

export function SalesOrderEditor() {
  const { id } = useParams<{ id: string }>();
  const api = useOrdersApi();
  const inventory = useInventoryApi();
  const billing = useBillingApi();
  const { customers } = useCustomers(true);
  const { confirm } = useDialogs();
  const navigate = useNavigate();

  const [order, setOrder] = useState<SalesOrder | null>(null);
  const [rows, setRows] = useState<OrderRow[]>([]);
  const [dirty, setDirty] = useState(false);
  const [customerId, setCustomerId] = useState("");
  const [expectedDate, setExpectedDate] = useState("");
  const [reference, setReference] = useState("");
  const [note, setNote] = useState("");

  const [products, setProducts] = useState<BillingProduct[]>([]);
  const [locations, setLocations] = useState<InvLocation[]>([]);
  const [deliveries, setDeliveries] = useState<Delivery[]>([]);
  const [invoices, setInvoices] = useState<SalesOrderInvoice[]>([]);

  const [loading, setLoading] = useState(id !== undefined);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [delivering, setDelivering] = useState(false);

  const nextRowKey = useRef(0);
  const freshKey = useCallback(() => {
    nextRowKey.current += 1;
    return `new-${nextRowKey.current}`;
  }, []);

  const adopt = useCallback((document: SalesOrder) => {
    setOrder(document);
    setRows(document.lines.map(orderRowFromLine));
    setCustomerId(document.customerId);
    setExpectedDate(document.expectedDate ?? "");
    setReference(document.reference);
    setNote(document.note);
    setDirty(false);
  }, []);

  useEffect(() => {
    if (id === undefined) {
      setRows([blankOrderRow("new-0")]);
      return;
    }
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const [document, sent, raised] = await Promise.all([
          api.salesOrder(id),
          api.deliveries(id),
          api.salesOrderInvoices(id),
        ]);
        if (!live) return;
        adopt(document);
        setDeliveries(sent);
        setInvoices(raised);
        setError(null);
      } catch (err) {
        if (live) setError(inventoryMessage(err, strings.inventoryOrderLoadFailed));
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [api, id, adopt]);

  useEffect(() => {
    let live = true;
    void Promise.all([billing.products(), inventory.locations()])
      .then(([catalog, places]) => {
        if (!live) return;
        setProducts(catalog);
        setLocations(places.filter((place) => !place.system && !place.archived));
      })
      .catch(() => {
        // The pickers are simply empty, which a person can see.
      });
    return () => {
      live = false;
    };
  }, [billing, inventory]);

  function patch(): OrderPatch | null {
    const lines = orderRowsDraft(rows);
    if (lines === null) return null;
    return {
      customerId,
      expectedDate: expectedDate === "" ? null : expectedDate,
      reference,
      note,
      lines,
    };
  }

  async function act(what: () => Promise<void>) {
    setBusy(true);
    setError(null);
    try {
      await what();
    } catch (err) {
      setError(inventoryMessage(err, strings.inventorySaveFailed));
    } finally {
      setBusy(false);
    }
  }

  async function save() {
    const body = patch();
    if (body === null) {
      setError(strings.inventoryFixLinesFirst);
      return;
    }
    if (customerId === "") {
      setError(strings.inventoryOrderNeedsCustomer);
      return;
    }
    await act(async () => {
      if (id === undefined) {
        const created = await api.createSalesOrder(body);
        adopt(created);
        await navigate(`/inventory/sales-orders/${created.id}`, { replace: true });
        return;
      }
      adopt(await api.updateSalesOrder(id, body));
      setNotice(null);
    });
  }

  async function markConfirmed() {
    if (id === undefined) return;
    if (
      !(await confirm({
        title: strings.inventoryConfirmOrder,
        message: strings.inventoryConfirmOrderConfirm,
        confirmLabel: strings.inventoryConfirmOrder,
      }))
    ) {
      return;
    }
    await act(async () => {
      adopt(await api.confirmSalesOrder(id));
    });
  }

  async function cancel() {
    if (id === undefined || order === null) return;
    const short = order.status === "partially_delivered";
    if (
      !(await confirm({
        title: strings.inventoryCancelOrder,
        message: short ? strings.inventoryCancelShortConfirm : strings.inventoryCancelOrderConfirm,
        confirmLabel: strings.inventoryCancelOrder,
        danger: true,
      }))
    ) {
      return;
    }
    await act(async () => {
      adopt(await api.cancelSalesOrder(id, short));
    });
  }

  async function discard() {
    if (id === undefined) return;
    if (
      !(await confirm({
        title: strings.inventoryDiscardDraft,
        message: strings.inventoryDiscardDraftConfirm,
        confirmLabel: strings.inventoryDiscardDraft,
        danger: true,
      }))
    ) {
      return;
    }
    await act(async () => {
      await api.deleteSalesOrder(id);
      await navigate("/inventory/sales-orders");
    });
  }

  async function raiseInvoice() {
    if (id === undefined) return;
    await act(async () => {
      const raised = await api.invoiceSalesOrder(id);
      adopt(raised.order);
      setInvoices(await api.salesOrderInvoices(id));
      setNotice(strings.inventoryInvoiceDrafted);
    });
  }

  const editable = isDraft(order);

  return (
    <div className={styles.page}>
      {/* On a phone the stylesheet stretched every toolbar button across the
          row it wraps onto; the child variant says the same thing once. */}
      <Toolbar
        label={strings.inventoryTabSales}
        className="max-[48rem]:[&>button]:flex-auto"
      >
        <button
          type="button"
          className={styles.linkAction}
          onClick={() => void navigate("/inventory/sales-orders")}
        >
          <ArrowLeft size={14} aria-hidden="true" /> {strings.inventoryBackToSalesOrders}
        </button>
        <h2 className={styles.documentTitle}>{order?.number ?? strings.inventoryDraftOrder}</h2>
        <span className="inline-flex flex-wrap items-center gap-2">
          {order !== null && (
            <StatusChip tone={soStatusTone(order.status)} label={soStatusLabel(order.status)} />
          )}
          {order?.late === true && <StatusChip tone="warn" label={strings.inventoryOrderLate} />}
        </span>
        <ToolbarSpacer />
        {(loading || busy) && <Spinner size={16} />}
        {editable && (
          <Button onClick={() => void save()} disabled={busy}>
            {id === undefined ? strings.inventoryCreateDraft : strings.inventorySaveDraft}
          </Button>
        )}
        {editable && id !== undefined && (
          <Button variant="ghost" onClick={() => void markConfirmed()} disabled={busy || dirty}>
            {strings.inventoryConfirmOrder}
          </Button>
        )}
        {order !== null && isOpen(order) && (
          <Button onClick={() => setDelivering(true)} disabled={busy}>
            {strings.inventoryDeliverGoods}
          </Button>
        )}
        {order !== null && hasBillable(order) && (
          <Button variant="ghost" onClick={() => void raiseInvoice()} disabled={busy}>
            {strings.inventoryRaiseInvoice}
          </Button>
        )}
        {order !== null && (order.status === "draft" ? id !== undefined : isOpen(order)) && (
          <Button
            variant="ghost"
            onClick={() => void (order.status === "draft" ? discard() : cancel())}
            disabled={busy}
          >
            {order.status === "draft" ? strings.inventoryDiscardDraft : strings.inventoryCancelOrder}
          </Button>
        )}
      </Toolbar>

      {error !== null && <ErrorBanner message={error} />}
      {notice !== null && <p className={styles.notice}>{notice}</p>}
      {order !== null && !editable && (
        <p className={styles.notice}>{strings.inventorySalesOrderFrozenNotice}</p>
      )}
      {editable && dirty && id !== undefined && (
        <p className={styles.notice}>{strings.inventoryUnsavedNotice}</p>
      )}

      <div className={styles.headerGrid}>
        <Field label={strings.inventoryColCustomer} hint={strings.inventoryCustomerHint}>
          {(control) =>
            editable ? (
              <Select
                id={control.id}
                aria-describedby={control["aria-describedby"]}
                fullWidth
                value={customerId}
                onChange={(e) => {
                  setCustomerId(e.target.value);
                  setDirty(true);
                }}
              >
                <option value="">{strings.inventoryPickCustomer}</option>
                {order !== null && !customers.some((person) => person.id === order.customerId) && (
                  <option value={order.customerId}>{order.customerName}</option>
                )}
                {customers
                  .filter((person) => !person.archived)
                  .map((person) => (
                    <option key={person.id} value={person.id}>
                      {person.name}
                    </option>
                  ))}
              </Select>
            ) : (
              <p className={styles.readOnlyValue}>{order?.customerName ?? ""}</p>
            )
          }
        </Field>

        <Field label={strings.inventoryColPromised} hint={strings.inventoryPromisedHint}>
          {(control) =>
            editable ? (
              <Input
                id={control.id}
                aria-describedby={control["aria-describedby"]}
                type="date"
                value={expectedDate}
                onChange={(e) => {
                  setExpectedDate(e.target.value);
                  setDirty(true);
                }}
              />
            ) : (
              <p className={styles.readOnlyValue}>{dayLabel(order?.expectedDate ?? null)}</p>
            )
          }
        </Field>

        <Field label={strings.inventoryFieldReference} hint={strings.inventoryReferenceHint}>
          {(control) =>
            editable ? (
              <Input
                id={control.id}
                aria-describedby={control["aria-describedby"]}
                value={reference}
                onChange={(e) => {
                  setReference(e.target.value);
                  setDirty(true);
                }}
              />
            ) : (
              <p className={styles.readOnlyValue}>{order?.reference ?? ""}</p>
            )
          }
        </Field>

        <Field label={strings.inventoryFieldConfirmed}>
          {() => <p className={styles.readOnlyValue}>{dayLabel(order?.confirmedDate ?? null)}</p>}
        </Field>
      </div>

      <Field label={strings.inventoryFieldNote} hint={strings.inventoryOrderNoteHint}>
        {(control) =>
          editable ? (
            <textarea
              id={control.id}
              aria-describedby={control["aria-describedby"]}
              className={styles.textarea}
              value={note}
              onChange={(e) => {
                setNote(e.target.value);
                setDirty(true);
              }}
              rows={2}
            />
          ) : (
            <p className={styles.readOnlyValue}>{order?.note ?? ""}</p>
          )
        }
      </Field>

      <OrderLines
        rows={rows}
        products={products}
        priceSide="sale"
        savedLines={order?.lines ?? []}
        saved={!dirty}
        currency={order?.currency ?? ""}
        readOnly={!editable}
        {...(order === null || editable
          ? {}
          : {
              progress: [
                {
                  key: "delivered",
                  label: strings.inventoryColDelivered,
                  values: order.lines.map((line) => qtyLabel(line.deliveredQtyMilli)),
                },
                {
                  key: "outstanding",
                  label: strings.inventoryColOutstanding,
                  values: order.lines.map((line) => qtyLabel(line.outstandingQtyMilli)),
                },
                {
                  key: "to-bill",
                  label: strings.inventoryColToBill,
                  values: order.lines.map((line) => qtyLabel(line.invoiceableQtyMilli)),
                },
              ],
            })}
        onChange={(next) => {
          setRows(next);
          setDirty(true);
        }}
        nextKey={freshKey}
      />

      {order !== null && (
        <TotalsPanel totals={order.totals} currency={order.currency} stale={dirty} />
      )}

      {order !== null && id !== undefined && order.status !== "draft" && (
        <>
          <FulfilmentList
            title={strings.inventoryConsignments}
            empty={strings.inventoryNoConsignments}
            entries={deliveries.map(
              (delivery): FulfilmentEntry => ({
                id: delivery.id,
                title: delivery.noteNumber ?? strings.inventoryConsignmentNo(delivery.sequenceNo),
                when: dayLabel(delivery.deliveredDate),
                place: `${delivery.locationCode} — ${delivery.locationName}`,
                note: delivery.note,
                lines: delivery.lines.map((line) => ({
                  key: line.moveId,
                  description: line.description,
                  qty: qtyLabel(line.qtyMilli),
                })),
              }),
            )}
          />

          <section className={styles.lines}>
            <h2 className={styles.sectionTitle}>{strings.inventoryRaisedInvoices}</h2>
            {invoices.length === 0 ? (
              <p className={styles.noMatches}>{strings.inventoryNoRaisedInvoices}</p>
            ) : (
              <ul className={styles.fulfilList}>
                {invoices.map((invoice) => (
                  <li key={invoice.id} className={styles.fulfilEntry}>
                    <div className={styles.fulfilHead}>
                      <button
                        type="button"
                        className={styles.rowName}
                        onClick={() =>
                          void navigate(`/billing/invoices/${invoice.invoiceId}`)
                        }
                      >
                        {invoice.invoiceNumber ?? strings.inventoryDraftInvoice}
                      </button>
                      <span className={styles.subtleInline}>{invoice.invoiceStatus}</span>
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </>
      )}

      {id !== undefined && <RecordHistory entityType="inventory.sales_order" entityId={id} />}

      {delivering && order !== null && id !== undefined && (
        <FulfilDialog
          title={strings.inventoryDeliverTitle(order.number ?? strings.inventoryDraftOrder)}
          subtitle={strings.inventoryDeliverSubtitle}
          locationLabel={strings.inventoryDeliverWhere}
          locationHint={strings.inventoryDeliverWhereHint}
          submitLabel={strings.inventoryBookConsignment}
          locations={locations}
          lines={order.lines.map((line) => ({
            lineId: line.id,
            description: line.description,
            unit: line.unit,
            outstandingQtyMilli: line.outstandingQtyMilli,
          }))}
          onSubmit={async (draft) => {
            const booked = await api.deliver(id, draft);
            adopt(booked.order);
            setDeliveries(await api.deliveries(id));
            setDelivering(false);
            setNotice(strings.inventoryConsignmentBooked);
          }}
          onClose={() => setDelivering(false)}
        />
      )}
    </div>
  );
}
