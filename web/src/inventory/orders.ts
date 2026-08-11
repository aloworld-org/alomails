// The client for the two order documents — what we buy and what we sell (alo
// Inventory, ADR 0035, wave B5.09b).
//
// Its own file rather than more methods on [`InventoryApi`](./api.ts), because
// the two clients answer different questions and change for different reasons:
// that one reads places, on-hand and the movement ledger and **writes nothing**,
// while this one raises, edits and advances documents. Both fail through the
// same `Problem` shape and share `InventoryError`, so a refusal reaches a person
// the same way whichever door it came from.
//
// **Nothing here computes money or quantities.** A line goes out as the integers
// the row model produced (milli-units, cents, basis points) and comes back with
// the server's own net and totals; what is outstanding on a line, what is
// invoiceable, and whether an order is late are all read, never derived. The one
// arithmetic-shaped thing this module does is send *less* than the whole: a
// partial receipt states quantities a person typed, and the server holds each of
// them to what is still owing.
//
// **The lifecycle is `POST`s, never fields.** Sending an order draws a number
// and writes the covering letter; confirming one freezes it; a consignment moves
// real stock. None of them may happen because a stale form was submitted, so
// none of them is reachable from the body of a `PATCH`.
import { useMemo } from "react";

import { useAuth } from "../auth";
import { getLocale } from "../i18n";
import { API_BASE } from "../platform/runtime";
import { problemDetail } from "../platform/rest";
import { InventoryError } from "./api";
import type {
  Delivery,
  FulfilmentDraft,
  OrderDraftMail,
  OrderPatch,
  PurchaseOrder,
  PurchaseOrderStatus,
  PurchaseOrderSummary,
  Receipt,
  SalesOrder,
  SalesOrderInvoice,
  SalesOrderStatus,
  SalesOrderSummary,
} from "./types";

type AuthorizedFetch = (input: string, init?: RequestInit) => Promise<Response>;

/** The two document kinds, as they appear in the path. */
type Kind = "purchase-orders" | "sales-orders";

/** The orders a tenant has placed and received. One instance per auth context. */
export class InventoryOrdersApi {
  readonly #fetch: AuthorizedFetch;

  constructor(authorizedFetch: AuthorizedFetch) {
    this.#fetch = authorizedFetch;
  }

  // ---- what we buy -------------------------------------------------------

  /** The tenant's purchase orders, newest first, without their lines. An
   *  unknown status is the server's `422`, never a widened filter. */
  purchaseOrders(status?: PurchaseOrderStatus): Promise<PurchaseOrderSummary[]> {
    return this.#read<{ purchaseOrders?: PurchaseOrderSummary[] }>(
      `/inventory/purchase-orders${statusQuery(status)}`,
    ).then((r) => r.purchaseOrders ?? []);
  }

  /** One order with its lines and totals. */
  purchaseOrder(id: string): Promise<PurchaseOrder> {
    return this.#read<{ purchaseOrder: PurchaseOrder }>(path("purchase-orders", id)).then(
      (r) => r.purchaseOrder,
    );
  }

  /** Raise a draft order. `supplierId` is the one thing it cannot be raised
   *  without; the currency falls back to the supplier's own. */
  createPurchaseOrder(patch: OrderPatch): Promise<PurchaseOrder> {
    return this.#write<{ purchaseOrder: PurchaseOrder }>(
      "/inventory/purchase-orders",
      "POST",
      patch,
    ).then((r) => r.purchaseOrder);
  }

  /** Edit a draft. Stated fields are merged onto the stored ones; a stated line
   *  set replaces the whole set in the order sent. */
  updatePurchaseOrder(id: string, patch: OrderPatch): Promise<PurchaseOrder> {
    return this.#write<{ purchaseOrder: PurchaseOrder }>(
      path("purchase-orders", id),
      "PATCH",
      patch,
    ).then((r) => r.purchaseOrder);
  }

  /** Discard a draft. An order that has been placed is cancelled instead — the
   *  server refuses this with a `409`, and the screen does not offer it. */
  deletePurchaseOrder(id: string): Promise<void> {
    return this.#write<unknown>(path("purchase-orders", id), "DELETE").then(() => undefined);
  }

  /**
   * Place the order: draw its number, freeze it, and write the covering letter
   * with the printed order attached into the caller's own Drafts.
   *
   * One act, because a *sent* purchase order means "we have asked them" — the
   * letter is carried in the same transaction, and nothing is ever sent by this
   * call. The interface language goes with it: the paper the supplier receives
   * is written in the language of whoever placed the order.
   */
  sendPurchaseOrder(id: string): Promise<{ order: PurchaseOrder; draft: OrderDraftMail }> {
    return this.#write<{ purchaseOrder: PurchaseOrder; draft: OrderDraftMail }>(
      `${path("purchase-orders", id)}/send?lang=${encodeURIComponent(getLocale())}`,
      "POST",
    ).then((r) => ({ order: r.purchaseOrder, draft: r.draft }));
  }

  /** Stop expecting the goods. A part-delivered order needs `shortClose`, which
   *  accepts what arrived as the whole of it; without it the server refuses. */
  cancelPurchaseOrder(id: string, shortClose = false): Promise<PurchaseOrder> {
    return this.#write<{ purchaseOrder: PurchaseOrder }>(
      `${path("purchase-orders", id)}/cancel`,
      "POST",
      { shortClose },
    ).then((r) => r.purchaseOrder);
  }

  /** The arrivals booked against one order, oldest first. */
  receipts(id: string): Promise<Receipt[]> {
    return this.#read<{ receipts?: Receipt[] }>(`${path("purchase-orders", id)}/receipts`).then(
      (r) => r.receipts ?? [],
    );
  }

  /** Book an arrival: goods move into the place named, the order advances, and
   *  a **draft** bill is raised for what came — one transaction on the server,
   *  so a caller gets all three or none. */
  receive(
    id: string,
    draft: FulfilmentDraft,
  ): Promise<{ order: PurchaseOrder; receipt: Receipt; billId: string }> {
    return this.#write<{ purchaseOrder: PurchaseOrder; receipt: Receipt; billId: string }>(
      `${path("purchase-orders", id)}/receipts`,
      "POST",
      draft,
    ).then((r) => ({ order: r.purchaseOrder, receipt: r.receipt, billId: r.billId }));
  }

  /** The printed order, as HTML the browser prints without leaving the app. */
  purchaseOrderHtml(id: string): Promise<string> {
    return this.#text(
      `${path("purchase-orders", id)}/print?lang=${encodeURIComponent(getLocale())}`,
    );
  }

  // ---- what we sell ------------------------------------------------------

  /** The tenant's sales orders, newest first, without their lines. */
  salesOrders(status?: SalesOrderStatus): Promise<SalesOrderSummary[]> {
    return this.#read<{ salesOrders?: SalesOrderSummary[] }>(
      `/inventory/sales-orders${statusQuery(status)}`,
    ).then((r) => r.salesOrders ?? []);
  }

  /** One order with its lines, its totals, and what each line can still bill. */
  salesOrder(id: string): Promise<SalesOrder> {
    return this.#read<{ salesOrder: SalesOrder }>(path("sales-orders", id)).then(
      (r) => r.salesOrder,
    );
  }

  /** Raise a draft order for a customer. */
  createSalesOrder(patch: OrderPatch): Promise<SalesOrder> {
    return this.#write<{ salesOrder: SalesOrder }>("/inventory/sales-orders", "POST", patch).then(
      (r) => r.salesOrder,
    );
  }

  /** Edit a draft. Same merge rules as a purchase order's. */
  updateSalesOrder(id: string, patch: OrderPatch): Promise<SalesOrder> {
    return this.#write<{ salesOrder: SalesOrder }>(path("sales-orders", id), "PATCH", patch).then(
      (r) => r.salesOrder,
    );
  }

  /** Discard a draft. */
  deleteSalesOrder(id: string): Promise<void> {
    return this.#write<unknown>(path("sales-orders", id), "DELETE").then(() => undefined);
  }

  /** Confirm the order: draw its number and freeze it. It writes no mail —
   *  confirming records an answer the customer already has. */
  confirmSalesOrder(id: string): Promise<SalesOrder> {
    return this.#write<{ salesOrder: SalesOrder }>(
      `${path("sales-orders", id)}/confirm`,
      "POST",
    ).then((r) => r.salesOrder);
  }

  /** Give up on the order; a part-delivered one needs `shortClose`. */
  cancelSalesOrder(id: string, shortClose = false): Promise<SalesOrder> {
    return this.#write<{ salesOrder: SalesOrder }>(`${path("sales-orders", id)}/cancel`, "POST", {
      shortClose,
    }).then((r) => r.salesOrder);
  }

  /** The consignments sent against one order, oldest first. */
  deliveries(id: string): Promise<Delivery[]> {
    return this.#read<{ deliveries?: Delivery[] }>(`${path("sales-orders", id)}/deliveries`).then(
      (r) => r.deliveries ?? [],
    );
  }

  /** Book a consignment: goods leave the place named and the order advances. */
  deliver(id: string, draft: FulfilmentDraft): Promise<{ order: SalesOrder; delivery: Delivery }> {
    return this.#write<{ salesOrder: SalesOrder; delivery: Delivery }>(
      `${path("sales-orders", id)}/deliveries`,
      "POST",
      draft,
    ).then((r) => ({ order: r.salesOrder, delivery: r.delivery }));
  }

  /** The invoices raised from one order. */
  salesOrderInvoices(id: string): Promise<SalesOrderInvoice[]> {
    return this.#read<{ invoices?: SalesOrderInvoice[] }>(
      `${path("sales-orders", id)}/invoices`,
    ).then((r) => r.invoices ?? []);
  }

  /** Raise a **draft** invoice for what has gone out and not yet been billed.
   *  It carries no number and is issued, if ever, by a person in Billing. */
  invoiceSalesOrder(id: string): Promise<{ order: SalesOrder; invoice: SalesOrderInvoice }> {
    return this.#write<{ salesOrder: SalesOrder; invoice: SalesOrderInvoice }>(
      `${path("sales-orders", id)}/invoice`,
      "POST",
    ).then((r) => ({ order: r.salesOrder, invoice: r.invoice }));
  }

  // ---- plumbing ----------------------------------------------------------

  async #read<T>(url: string): Promise<T> {
    const res = await this.#send(url, { method: "GET" });
    if (!res.ok) throw await failure(res);
    return (await res.json()) as T;
  }

  /** A `GET` whose body is not JSON — the printed order. A failure still
   *  carries the server's `Problem` detail, which is. */
  async #text(url: string): Promise<string> {
    const res = await this.#send(url, { method: "GET" });
    if (!res.ok) throw await failure(res);
    return res.text();
  }

  async #write<T>(url: string, method: string, body?: unknown): Promise<T> {
    const res = await this.#send(url, {
      method,
      ...(body === undefined
        ? {}
        : { headers: { "content-type": "application/json" }, body: JSON.stringify(body) }),
    });
    if (!res.ok) throw await failure(res);
    return (await res.json()) as T;
  }

  async #send(url: string, init: RequestInit): Promise<Response> {
    try {
      return await this.#fetch(`${API_BASE}${url}`, init);
    } catch {
      // A dropped connection is not a status code; give it one the UI can
      // treat like any other failure rather than an unhandled rejection.
      throw new InventoryError(0, null);
    }
  }
}

/** One document's path, with an id that may contain anything. */
function path(kind: Kind, id: string): string {
  return `/inventory/${kind}/${encodeURIComponent(id)}`;
}

/** The status filter, or nothing at all. "Everything" is an absent parameter
 *  rather than an empty one — the server would accept either, and a URL that
 *  says what it asks for is the one worth reading in a log. */
function statusQuery(status?: string): string {
  return status === undefined || status === "" ? "" : `?status=${encodeURIComponent(status)}`;
}

async function failure(res: Response): Promise<InventoryError> {
  return new InventoryError(res.status, await problemDetail(res));
}

/** The orders client bound to the current session. Memoized per auth context,
 *  so effects keyed on it do not loop. */
export function useOrdersApi(): InventoryOrdersApi {
  const { authorizedFetch } = useAuth();
  return useMemo(() => new InventoryOrdersApi(authorizedFetch), [authorizedFetch]);
}
