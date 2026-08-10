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
// rather than declaring a second one.

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

/** A supplier, as the catalog's default-supplier picker needs one. The full
 *  record is the suppliers screen's business (B5.09b); a picker needs a name
 *  and an id. */
export interface InvSupplier {
  id: string;
  name: string;
  archived: boolean;
}
