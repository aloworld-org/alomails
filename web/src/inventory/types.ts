// The shapes the `/inventory` API sends, as TypeScript sees them (alo
// Inventory, ADR 0035, wave B5).
//
// Two rules hold across every type here, and they are the module's whole
// honesty story:
//
//   1. **Quantities are integer milli-units** (`qtyMilli`), never floats, for
//      the reason a document line is: a third of a kilo has to survive being
//      stored and read back. Money is integer cents, always.
//   2. **Nothing here is derived in the browser.** On-hand is the server's fold
//      of the movement ledger, the reference value is the server's arithmetic
//      at the product's purchase price, and a location's kind is the server's
//      word for it. A field this file does not carry is a field no screen may
//      invent (`docs/design/inventory.md`).
//
// The product record itself is Billing's (`BillingProduct`): a product is one
// row with a price half and a warehouse half, and this module reads that type
// rather than declaring a second one. So are a document's totals and a line on
// its way to the server (`DocumentTotals`, `LineDraft`): an order's money is
// the same money, computed by the same server code, and a second declaration of
// it here would be a second thing to keep in step.
import type { DocumentTotals, LineDraft } from "../billing";

/**
 * What a place *is*.
 *
 * The four virtual kinds are counterparties, not rooms: a receipt moves goods
 * from `supplier`, a delivery moves them to `customer`, an adjustment squares
 * them against `adjust`, and `production` is reserved for the wave that builds
 * things. They are what makes every movement have two ends and every balance
 * sum to zero — and they are why a stock screen leaves them out by default,
 * since `supplier` holding minus four hundred is an accounting fact and not a
 * shelf.
 */
export type LocationKind =
  | "stock"
  | "transit"
  | "supplier"
  | "customer"
  | "adjust"
  | "production";

/** A place goods can be, as the server stores it. */
export interface InvLocation {
  id: string;
  /** The tenant's short code, unique among their places ("MAIN", "VAN1"). */
  code: string;
  name: string;
  kind: LocationKind;
  /** `true` for the four virtual counterparties: seeded, never editable, and
   *  not somewhere a person puts anything. */
  system: boolean;
  archived: boolean;
  archivedAt: string | null;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
}

/**
 * One product at one place: how much is there, and what that much is worth at
 * the product's purchase price.
 *
 * `valueCents` is a **reference figure, not a balance**. B5 posts nothing to
 * the journal and chooses no costing method (`docs/design/inventory.md` § Out
 * of scope), so this number is what the goods cost us at today's purchase
 * price and is labelled as such on every screen that shows it.
 */
export interface StockLevel {
  productId: string;
  productName: string;
  sku: string;
  locationId: string;
  locationCode: string;
  locationName: string;
  locationKind: LocationKind;
  /** `false` for the virtual counterparties, where a negative quantity is
   *  correct rather than alarming. */
  real: boolean;
  qtyMilli: number;
  valueCents: number;
  /** When something last moved here; `null` for a row that has never moved. */
  lastMoveAt: string | null;
}

/** The stock read: the rows, and the server's sum of exactly those rows. */
export interface StockRead {
  stock: StockLevel[];
  totalValueCents: number;
}

/** Why a movement happened. The server's vocabulary, which a screen shows and
 *  never invents: an unknown one — a reason a newer server learned first — is
 *  displayed verbatim rather than blanked. */
export type MoveReason =
  | "receipt"
  | "delivery"
  | "transfer"
  | "adjustment"
  | "return"
  | "shrinkage"
  | "count";

/** The reason code a person picked for a manual adjustment; `null` for the
 *  movements a document made. */
export type AdjustReasonCode =
  | "damaged"
  | "lost"
  | "found"
  | "expired"
  | "theft"
  | "sample"
  | "correction";

/**
 * One movement of the ledger: what moved, from where, to where, how much, why,
 * and which document caused it.
 *
 * This row is the reason the stock screen has no editable quantity field. A
 * person who reads one of these once never asks to type a new number into an
 * on-hand column — the answer to "where did the other four go" is *here*.
 */
export interface InvMove {
  id: string;
  productId: string;
  productName: string;
  fromLocationId: string;
  fromCode: string;
  fromName: string;
  toLocationId: string;
  toCode: string;
  toName: string;
  qtyMilli: number;
  reason: MoveReason;
  reasonCode: AdjustReasonCode | null;
  note: string | null;
  /** What caused it — `po`, `so`, `count` … — `null` for a manual movement. */
  refKind: string | null;
  refId: string | null;
  occurredAt: string;
  createdBy: string;
  createdAt: string;
}

/** A supplier, as the catalog's default-supplier picker and the purchase-order
 *  header need one. The full supplier record — addresses, terms, lead times —
 *  is a screen this wave has not built; what a picker needs is a name and an
 *  id, and what an order header needs is the same pair. */
export interface InvSupplier {
  id: string;
  name: string;
  archived: boolean;
}

// ---- the two order documents (B5.09b) ------------------------------------
//
// A purchase order and a sales order are mirror images: the same header, the
// same lines, the same totals, pointed at a supplier or at a customer. They are
// typed separately all the same, because the two things a screen must never
// confuse are *which way the goods go* and *what a quantity on a line has
// already done* — received against ordered on one, delivered and invoiced
// against ordered on the other.
//
// Every quantity below is integer milli-units and every amount integer cents,
// and **each one arrives computed**: outstanding, invoiceable, the line's net
// and the document's totals are the server's, and no screen in this module
// subtracts one from another (`docs/design/inventory.md`).

/** What both documents' lines carry. Shape-identical to a billing line, which
 *  is what it is: the order line model is the document line model, plus the
 *  catalog link that lets a receipt or a delivery move real goods. */
export interface OrderLine {
  id: string;
  /** The catalog item this line moves, or `null` for a charge in words —
   *  freight, assembly — which no consignment ever carries. */
  productId: string | null;
  description: string;
  unit: string;
  qtyMilli: number;
  unitPriceCents: number;
  vatRateBp: number;
  netCents: number;
}

/** An ordered line, and how much of it has arrived. */
export interface PurchaseOrderLine extends OrderLine {
  receivedQtyMilli: number;
  /** Still to come. `0` on a charge in words, which must not hold an order
   *  open. */
  outstandingQtyMilli: number;
}

/** A sold line, and how much of it has gone out and been billed. */
export interface SalesOrderLine extends OrderLine {
  deliveredQtyMilli: number;
  /** Still to go. */
  outstandingQtyMilli: number;
  invoicedQtyMilli: number;
  /** What an invoice raised **right now** would take from this line — the
   *  store's own rule, not a subtraction, so the number a screen offers and the
   *  number the button bills are the same one. */
  invoiceableQtyMilli: number;
}

/** Where an order we placed has got to. */
export type PurchaseOrderStatus =
  | "draft"
  | "sent"
  | "partially_received"
  | "received"
  | "cancelled";

/** Where an order a customer placed has got to. */
export type SalesOrderStatus =
  | "draft"
  | "confirmed"
  | "partially_delivered"
  | "delivered"
  | "cancelled";

/** A purchase order without its lines — what the list reads. */
export interface PurchaseOrderSummary {
  id: string;
  supplierId: string;
  supplierName: string;
  status: PurchaseOrderStatus;
  currency: string;
  /** `null` until the order is placed: a draft has drawn no number. */
  number: string | null;
  /** The day it was placed (`YYYY-MM-DD`), `null` while it is a draft. */
  orderedDate: string | null;
  expectedDate: string | null;
  /** The day it was received or given up on. */
  closedDate: string | null;
  /** The server's own reading of "past the day we expected them", computed
   *  against the server's today and never against the browser's. */
  late: boolean;
  reference: string;
  note: string;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
  totals: DocumentTotals;
}

/** A whole purchase order: the header, its lines in print order, its totals. */
export interface PurchaseOrder extends PurchaseOrderSummary {
  lines: PurchaseOrderLine[];
}

/** A sales order without its lines. */
export interface SalesOrderSummary {
  id: string;
  customerId: string;
  customerName: string;
  status: SalesOrderStatus;
  currency: string;
  number: string | null;
  /** The day we confirmed it, `null` while it is a draft. */
  confirmedDate: string | null;
  expectedDate: string | null;
  closedDate: string | null;
  late: boolean;
  reference: string;
  note: string;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
  totals: DocumentTotals;
}

/** A whole sales order. */
export interface SalesOrder extends SalesOrderSummary {
  lines: SalesOrderLine[];
}

/** One line of a consignment, either direction: what moved, and the ledger row
 *  that recorded it. */
export interface FulfilmentLine {
  lineId: string;
  productId: string | null;
  description: string;
  qtyMilli: number;
  moveId: string;
}

/** An arrival booked against a purchase order. */
export interface Receipt {
  id: string;
  /** Its place in the order's own sequence, from one. */
  sequenceNo: number;
  locationId: string;
  locationCode: string;
  locationName: string;
  receivedDate: string;
  note: string;
  /** The draft bill this arrival raised; `null` when that bill has since been
   *  thrown away, which is a thing a person may do to an undecided bill. */
  billId: string | null;
  createdBy: string;
  createdAt: string;
  lines: FulfilmentLine[];
}

/** A consignment booked against a sales order. */
export interface Delivery {
  id: string;
  sequenceNo: number;
  /** The delivery note's number, `null` for a consignment against an order
   *  that has none. */
  noteNumber: string | null;
  locationId: string;
  locationCode: string;
  locationName: string;
  deliveredDate: string;
  note: string;
  createdBy: string;
  createdAt: string;
  lines: (FulfilmentLine & { unit: string })[];
}

/** An invoice raised from a sales order, and where that invoice has got to. */
export interface SalesOrderInvoice {
  id: string;
  invoiceId: string;
  /** `null` while the invoice is still a draft — it has consumed nothing from
   *  the gapless series. */
  invoiceNumber: string | null;
  invoiceStatus: string;
  createdBy: string;
  createdAt: string;
  lines: { lineId: string; qtyMilli: number }[];
}

/** The covering letter a placed order wrote: a draft in the user's own Drafts,
 *  never a sent message. */
export interface OrderDraftMail {
  id: string;
  to: string;
  subject: string;
  attachment: { name: string; sizeBytes: number };
}

/** What a client states about a line when booking a consignment. Absent lines
 *  altogether mean "everything still outstanding"; an empty list is refused by
 *  the server rather than widened into that. */
export interface FulfilmentLineDraft {
  lineId: string;
  qtyMilli: number;
}

/** What a client states when booking one. */
export interface FulfilmentDraft {
  locationId: string;
  lines?: FulfilmentLineDraft[];
  note?: string;
}

/** A line on its way to the server. It carries the catalog link (what makes
 *  goods move later) and nothing derived. */
export interface OrderLineDraft extends LineDraft {
  /** `""` for a charge in words — the server reads a blank id as no product. */
  productId: string;
}

/** The writable parts of an order header. Every field is optional: the same
 *  body raises a document and edits one, and an absent field is left alone
 *  rather than blanked. */
export interface OrderPatch {
  supplierId?: string;
  customerId?: string;
  /** `null` clears the expectation; absent leaves it as it was. */
  expectedDate?: string | null;
  reference?: string;
  note?: string;
  lines?: OrderLineDraft[];
}
