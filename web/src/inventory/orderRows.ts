// The editing model of an order's lines, and of the quantities on a
// consignment sheet (B5.09b). Pure: no React, no network, no formatting.
//
// **The rules are Billing's.** What makes a typed row into a line — a
// description that is not blank, a quantity that parses, a blank quantity
// meaning one — was settled when invoices were built, and this module imports
// those rules rather than restating them (`web/src/billing/lineRows.ts`). A
// second, slightly different idea of what a usable row is would show as an order
// and its invoice disagreeing about a third of a kilo.
//
// **What is new here is the catalog link.** An order line may name a product,
// and that link is not decoration: it is what a receipt turns into goods moving
// into a place, and what a delivery turns into goods leaving one. So the row
// carries it, picking a product sets it, and clearing the description never
// silently keeps it pointing at something else.
//
// **A product is copied onto a row at the price of the direction it is going.**
// A purchase order takes the purchase price, a sales order the sale price —
// both as a snapshot of this moment, exactly as the store models a line, so
// editing the price list afterwards never reaches back into a document.
import {
  blankRow,
  hundredthsToInput,
  isBlankRow,
  milliToInput,
  parseMilli,
  rowDraft,
  rowProblem,
  type BillingProduct,
  type LineRow,
} from "../billing";
import type { OrderLine, OrderLineDraft } from "./types";

/** One row of an order's line grid: a billing row, plus the catalog item it
 *  moves. `""` is "no product" — a charge in words, which no consignment ever
 *  carries. */
export interface OrderRow extends LineRow {
  productId: string;
}

/** Which price a picked product is copied at. The direction the goods go, not
 *  a preference: an order we place is priced at what we pay. */
export type PriceSide = "purchase" | "sale";

/** A row with nothing in it, ready to be typed into. */
export function blankOrderRow(key: string): OrderRow {
  return { ...blankRow(key), productId: "" };
}

/** A stored line as an editable row. */
export function orderRowFromLine(line: OrderLine): OrderRow {
  return {
    key: line.id,
    productId: line.productId ?? "",
    description: line.description,
    unit: line.unit,
    qty: milliToInput(line.qtyMilli),
    price: hundredthsToInput(line.unitPriceCents),
    rate: hundredthsToInput(line.vatRateBp),
  };
}

/**
 * A catalog item copied onto a row: what it is called, its unit, its price on
 * the side of the trade this document is on, and its VAT rate — all as they are
 * **now**, which is what the document will carry for ever.
 *
 * The quantity is the row's own: picking an item does not decide how many.
 */
export function orderRowFromProduct(
  row: OrderRow,
  product: BillingProduct,
  side: PriceSide,
): OrderRow {
  return {
    key: row.key,
    productId: product.id,
    description: product.name,
    unit: product.unit,
    qty: row.qty,
    price: hundredthsToInput(
      side === "purchase" ? product.purchasePriceCents : product.unitPriceCents,
    ),
    rate: hundredthsToInput(product.vatRateBp),
  };
}

/**
 * The whole line set to send, or `null` when a row that is not blank cannot be
 * turned into a line.
 *
 * Wholly blank rows are dropped — adding a row and changing your mind is not an
 * edit — and everything else must be a line, because the API replaces the set
 * in one write: a single unusable row has to stop the save rather than quietly
 * take the line it stands for out of the order.
 */
export function orderRowsDraft(rows: OrderRow[]): OrderLineDraft[] | null {
  const drafts: OrderLineDraft[] = [];
  for (const row of rows) {
    if (isBlankRow(row)) continue;
    const draft = rowDraft(row);
    if (draft === null) return null;
    drafts.push({ ...draft, productId: row.productId });
  }
  return drafts;
}

/** The first thing standing between an order row and being a line, if
 *  anything. A blank row has no problem: it is simply not a line yet. */
export function orderRowProblem(row: OrderRow): ReturnType<typeof rowProblem> {
  return isBlankRow(row) ? null : rowProblem(row);
}

/** One line of a consignment as it is being typed: how much of this order line
 *  arrived, or went out. */
export interface FulfilRow {
  lineId: string;
  /** As typed. Blank is not zero — it is a line this consignment says nothing
   *  about, and it is left out of what is sent. */
  qty: string;
}

/**
 * What a consignment sheet asks the server to book, or `null` if a stated
 * quantity is not a quantity.
 *
 * An empty result is a real answer and a different one from `null`: it means
 * every row was left blank, which is a sheet that books nothing. The caller
 * decides what to do with that — for both documents the server refuses an empty
 * set rather than widening it to "everything", and the screen says so before
 * asking.
 *
 * **Nothing is held against what is outstanding here.** That rule is the
 * store's, under the row lock that books the movement; a browser that also
 * enforced it would be a second opinion, and the two would disagree the moment
 * a colleague booked a consignment in another tab.
 */
export function fulfilDraft(rows: FulfilRow[]): { lineId: string; qtyMilli: number }[] | null {
  const lines: { lineId: string; qtyMilli: number }[] = [];
  for (const row of rows) {
    if (row.qty.trim() === "") continue;
    const qtyMilli = parseMilli(row.qty);
    if (qtyMilli === null) return null;
    // Zero is stated nothing, not booked nothing: a row typed down to zero is
    // the same as a row left alone, and sending it would ask the server to
    // record a movement of no goods.
    if (qtyMilli === 0) continue;
    lines.push({ lineId: row.lineId, qtyMilli });
  }
  return lines;
}
