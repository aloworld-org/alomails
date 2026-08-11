// One purchase order, from the draft nobody has seen to the goods on the shelf
// (B5.09b).
//
// The screen has three lives, and the document decides which one it is in:
//
//  1. **A draft** — a header and a line grid, editable, saved when a person
//     says so. It has drawn no number and been shown to nobody.
//  2. **Placed** — frozen, wearing the number the supplier now holds, with the
//     one act left that matters: booking what arrives, line by line, until
//     nothing is outstanding.
//  3. **Finished with** — received or given up on, read-only for ever, with its
//     arrivals still listed underneath.
//
// **Placing it is one act and the button says so.** `POST …/send` draws the
// number, freezes the order *and* writes the covering letter with the printed
// order attached into the caller's own Drafts — never sending it. The dialog
// says all three things before it happens, because a number drawn is a number
// spent: the series is gapless, and there is no undo.
//
// **Nothing on this screen decides a rule.** Whether an edit is allowed, whether
// a quantity exceeds what is owing, whether giving up on a part-received order
// needs the shortfall accepted — every one of those is the store's, under the
// row lock of the write itself, and what a person sees when one of them refuses
// is the server's own sentence.
import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { ArrowLeft, Printer } from "lucide-react";

import { RecordHistory } from "../audit";
import { TotalsPanel, printSheet, useBillingApi, type BillingProduct } from "../billing";
import { Button, Spinner, useDialogs } from "../ds";
import { strings } from "../i18n";
import { inventoryMessage, useInventoryApi } from "./api";
import { dayLabel, poStatusLabel, poStatusTone, qtyLabel } from "./format";
import { FulfilDialog } from "./FulfilDialog";
import { FulfilmentList, type FulfilmentEntry } from "./Fulfilments";
import { OrderLines } from "./OrderLines";
import { blankOrderRow, orderRowFromLine, orderRowsDraft, type OrderRow } from "./orderRows";
import { useOrdersApi } from "./orders";
import { ErrorBanner, Field, StatusChip } from "./parts";
import type { InvLocation, InvSupplier, OrderPatch, PurchaseOrder, Receipt } from "./types";
import styles from "./InventoryModule.module.css";

/** Whether the document can still be edited. The store decides the same thing
 *  under the row lock; this is that decision read early, so a screen offers no
 *  field that a save would refuse. */
function isDraft(order: PurchaseOrder | null): boolean {
  return order === null || order.status === "draft";
}

/** Whether goods can still arrive against it. */
function isOpen(order: PurchaseOrder): boolean {
  return order.status === "sent" || order.status === "partially_received";
}

export function PurchaseOrderEditor() {
  const { id } = useParams<{ id: string }>();
  const api = useOrdersApi();
  const inventory = useInventoryApi();
  const billing = useBillingApi();
  const { confirm } = useDialogs();
  const navigate = useNavigate();

  const [order, setOrder] = useState<PurchaseOrder | null>(null);
  const [rows, setRows] = useState<OrderRow[]>([]);
  /** Whether the rows and the header say what the server last stored. Kept as
   *  an explicit flag rather than compared field by field: the moment it is
   *  false the totals beside the lines are yesterday's, and they are dimmed. */
  const [dirty, setDirty] = useState(false);
  const [supplierId, setSupplierId] = useState("");
  const [expectedDate, setExpectedDate] = useState("");
  const [reference, setReference] = useState("");
  const [note, setNote] = useState("");

  const [suppliers, setSuppliers] = useState<InvSupplier[]>([]);
  const [products, setProducts] = useState<BillingProduct[]>([]);
  const [locations, setLocations] = useState<InvLocation[]>([]);
  const [receipts, setReceipts] = useState<Receipt[]>([]);

  const [loading, setLoading] = useState(id !== undefined);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** Something that happened and is worth saying once — the letter that is now
   *  in Drafts, the bill an arrival raised. Not an error, so it never borrows
   *  the error colour. */
  const [notice, setNotice] = useState<string | null>(null);
  const [receiving, setReceiving] = useState(false);

  /** Row identity, from the screen that owns the document. A counter, because
   *  two fresh rows must never collide and a stored line's own id is only
   *  available for lines the server has seen. */
  const nextRowKey = useRef(0);
  const freshKey = useCallback(() => {
    nextRowKey.current += 1;
    return `new-${nextRowKey.current}`;
  }, []);

  /** Takes the document the server just answered with as the truth: header
   *  fields, rows and totals all come from it, and the screen is clean again. */
  const adopt = useCallback((document: PurchaseOrder) => {
    setOrder(document);
    setRows(document.lines.map(orderRowFromLine));
    setSupplierId(document.supplierId);
    setExpectedDate(document.expectedDate ?? "");
    setReference(document.reference);
    setNote(document.note);
    setDirty(false);
  }, []);

  useEffect(() => {
    if (id === undefined) {
      // A new order starts with one empty row: an order with no lines is not a
      // thing anybody wants, and a person who opened this screen came to type.
      setRows([blankOrderRow("new-0")]);
      return;
    }
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        const [document, arrivals] = await Promise.all([api.purchaseOrder(id), api.receipts(id)]);
        if (!live) return;
        adopt(document);
        setReceipts(arrivals);
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

  // The pickers, kept apart from the document's own read: a catalog that fails
  // to load must not blank an order, and a grid with no picker still types.
  useEffect(() => {
    let live = true;
    void Promise.all([inventory.suppliers(), billing.products(), inventory.locations()])
      .then(([supplierList, catalog, places]) => {
        if (!live) return;
        setSuppliers(supplierList.filter((supplier) => !supplier.archived));
        setProducts(catalog);
        setLocations(places.filter((place) => !place.system && !place.archived));
      })
      .catch(() => {
        // Nothing to say: the pickers are simply empty, which a person can see.
      });
    return () => {
      live = false;
    };
  }, [inventory, billing]);

  /** The header and lines as the API takes them. `null` when a row that is not
   *  blank cannot be a line — the grid is already saying which one. */
  function patch(): OrderPatch | null {
    const lines = orderRowsDraft(rows);
    if (lines === null) return null;
    return {
      supplierId,
      // A blank box is "no date expected", which the contract spells `null`;
      // the empty string is refused by the server, and rightly.
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
    if (supplierId === "") {
      setError(strings.inventoryOrderNeedsSupplier);
      return;
    }
    await act(async () => {
      if (id === undefined) {
        const created = await api.createPurchaseOrder(body);
        adopt(created);
        // Replaces the /new entry, so Back goes to the list rather than to a
        // form for a document that now exists.
        await navigate(`/inventory/purchase-orders/${created.id}`, { replace: true });
        return;
      }
      adopt(await api.updatePurchaseOrder(id, body));
      setNotice(null);
    });
  }

  async function send() {
    if (id === undefined || order === null) return;
    if (
      !(await confirm({
        title: strings.inventorySendOrder,
        message: strings.inventorySendOrderConfirm,
        confirmLabel: strings.inventorySendOrder,
      }))
    ) {
      return;
    }
    await act(async () => {
      const placed = await api.sendPurchaseOrder(id);
      adopt(placed.order);
      setNotice(strings.inventoryOrderPlacedNotice(placed.draft.to, placed.draft.attachment.name));
    });
  }

  async function cancel() {
    if (id === undefined || order === null) return;
    const short = order.status === "partially_received";
    if (
      !(await confirm({
        title: strings.inventoryCancelOrder,
        message: short
          ? strings.inventoryCancelShortConfirm
          : strings.inventoryCancelOrderConfirm,
        confirmLabel: strings.inventoryCancelOrder,
        danger: true,
      }))
    ) {
      return;
    }
    await act(async () => {
      adopt(await api.cancelPurchaseOrder(id, short));
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
      await api.deletePurchaseOrder(id);
      await navigate("/inventory/purchase-orders");
    });
  }

  async function print() {
    if (id === undefined) return;
    await act(async () => {
      printSheet(await api.purchaseOrderHtml(id));
    });
  }

  const editable = isDraft(order);
  const draftOnlyLines = order?.lines ?? [];

  return (
    <div className={styles.page}>
      <div className={styles.toolbar}>
        <button
          type="button"
          className={styles.linkAction}
          onClick={() => void navigate("/inventory/purchase-orders")}
        >
          <ArrowLeft size={14} aria-hidden="true" /> {strings.inventoryBackToPurchaseOrders}
        </button>
        <h2 className={styles.documentTitle}>
          {order?.number ?? strings.inventoryDraftOrder}
        </h2>
        <span className={styles.chips}>
          {order !== null && (
            <StatusChip tone={poStatusTone(order.status)} label={poStatusLabel(order.status)} />
          )}
          {order?.late === true && <StatusChip tone="warn" label={strings.inventoryOrderLate} />}
        </span>
        <span className={styles.toolbarSpacer} />
        {(loading || busy) && <Spinner size={16} />}
        {id !== undefined && order !== null && !editable && (
          <Button variant="ghost" onClick={() => void print()} disabled={busy}>
            <Printer size={16} /> {strings.inventoryPrintOrder}
          </Button>
        )}
        {editable && (
          <Button onClick={() => void save()} disabled={busy}>
            {id === undefined ? strings.inventoryCreateDraft : strings.inventorySaveDraft}
          </Button>
        )}
        {editable && id !== undefined && (
          <Button variant="ghost" onClick={() => void send()} disabled={busy || dirty}>
            {strings.inventorySendOrder}
          </Button>
        )}
        {order !== null && isOpen(order) && (
          <Button onClick={() => setReceiving(true)} disabled={busy}>
            {strings.inventoryReceiveGoods}
          </Button>
        )}
        {order !== null && (order.status === "draft" ? id !== undefined : isOpen(order)) && (
          <Button
            variant="ghost"
            onClick={() => void (order.status === "draft" ? discard() : cancel())}
            disabled={busy}
          >
            {order.status === "draft"
              ? strings.inventoryDiscardDraft
              : strings.inventoryCancelOrder}
          </Button>
        )}
      </div>

      {error !== null && <ErrorBanner message={error} />}
      {notice !== null && <p className={styles.notice}>{notice}</p>}
      {/* Said where the button is: a placed order cannot be edited, and the
          reason is that the supplier holds the paper. */}
      {order !== null && !editable && (
        <p className={styles.notice}>{strings.inventoryOrderFrozenNotice}</p>
      )}
      {editable && dirty && id !== undefined && (
        <p className={styles.notice}>{strings.inventoryUnsavedNotice}</p>
      )}

      <div className={styles.headerGrid}>
        <Field label={strings.inventoryColSupplier} hint={strings.inventorySupplierHint}>
          {editable ? (
            <select
              className={styles.select}
              value={supplierId}
              onChange={(e) => {
                setSupplierId(e.target.value);
                setDirty(true);
              }}
            >
              <option value="">{strings.inventoryPickSupplier}</option>
              {/* The stored supplier is always offered, even if they have since
                  been archived: a document must be able to say who it is with. */}
              {order !== null &&
                !suppliers.some((supplier) => supplier.id === order.supplierId) && (
                  <option value={order.supplierId}>{order.supplierName}</option>
                )}
              {suppliers.map((supplier) => (
                <option key={supplier.id} value={supplier.id}>
                  {supplier.name}
                </option>
              ))}
            </select>
          ) : (
            <p className={styles.readOnlyValue}>{order?.supplierName ?? ""}</p>
          )}
        </Field>

        <Field label={strings.inventoryColExpected} hint={strings.inventoryExpectedHint}>
          {editable ? (
            <input
              className={styles.input}
              type="date"
              value={expectedDate}
              onChange={(e) => {
                setExpectedDate(e.target.value);
                setDirty(true);
              }}
            />
          ) : (
            <p className={styles.readOnlyValue}>{dayLabel(order?.expectedDate ?? null)}</p>
          )}
        </Field>

        <Field label={strings.inventoryFieldReference} hint={strings.inventoryReferenceHint}>
          {editable ? (
            <input
              className={styles.input}
              value={reference}
              onChange={(e) => {
                setReference(e.target.value);
                setDirty(true);
              }}
            />
          ) : (
            <p className={styles.readOnlyValue}>{order?.reference ?? ""}</p>
          )}
        </Field>

        <Field label={strings.inventoryFieldOrdered}>
          <p className={styles.readOnlyValue}>{dayLabel(order?.orderedDate ?? null)}</p>
        </Field>
      </div>

      <Field label={strings.inventoryFieldNote} hint={strings.inventoryOrderNoteHint}>
        {editable ? (
          <textarea
            className={`${styles.input} ${styles.textarea}`}
            value={note}
            onChange={(e) => {
              setNote(e.target.value);
              setDirty(true);
            }}
            rows={2}
          />
        ) : (
          <p className={styles.readOnlyValue}>{order?.note ?? ""}</p>
        )}
      </Field>

      <OrderLines
        rows={rows}
        products={products}
        priceSide="purchase"
        savedLines={draftOnlyLines}
        saved={!dirty}
        currency={order?.currency ?? ""}
        readOnly={!editable}
        {...(order === null || editable
          ? {}
          : {
              progress: [
                {
                  key: "received",
                  label: strings.inventoryColReceived,
                  values: order.lines.map((line) => qtyLabel(line.receivedQtyMilli)),
                },
                {
                  key: "outstanding",
                  label: strings.inventoryColOutstanding,
                  values: order.lines.map((line) => qtyLabel(line.outstandingQtyMilli)),
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
        <FulfilmentList
          title={strings.inventoryArrivals}
          empty={strings.inventoryNoArrivals}
          entries={receipts.map(
            (receipt): FulfilmentEntry => ({
              id: receipt.id,
              title: strings.inventoryArrivalNo(receipt.sequenceNo),
              when: dayLabel(receipt.receivedDate),
              place: `${receipt.locationCode} — ${receipt.locationName}`,
              note: receipt.note,
              lines: receipt.lines.map((line) => ({
                key: line.moveId,
                description: line.description,
                qty: qtyLabel(line.qtyMilli),
              })),
              ...(receipt.billId === null ? {} : { aside: strings.inventoryBillDrafted }),
            }),
          )}
        />
      )}

      {/* Who did what to this order, and when (B2.13). Below the document: a
          buyer reads the quantities first and the history only when something
          looks wrong. A draft that was never saved has no id and no history. */}
      {id !== undefined && <RecordHistory entityType="inventory.purchase_order" entityId={id} />}

      {receiving && order !== null && id !== undefined && (
        <FulfilDialog
          title={strings.inventoryReceiveTitle(order.number ?? strings.inventoryDraftOrder)}
          subtitle={strings.inventoryReceiveSubtitle}
          locationLabel={strings.inventoryReceiveWhere}
          locationHint={strings.inventoryReceiveWhereHint}
          submitLabel={strings.inventoryBookArrival}
          locations={locations}
          lines={order.lines.map((line) => ({
            lineId: line.id,
            description: line.description,
            unit: line.unit,
            outstandingQtyMilli: line.outstandingQtyMilli,
          }))}
          onSubmit={async (draft) => {
            const booked = await api.receive(id, draft);
            adopt(booked.order);
            setReceipts(await api.receipts(id));
            setReceiving(false);
            setNotice(strings.inventoryArrivalBooked);
          }}
          onClose={() => setReceiving(false)}
        />
      )}
    </div>
  );
}
