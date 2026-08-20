// The editing model of a document's lines: the bridge between what a person
// types in the line grid and the whole line set the API takes.
//
// It is pure — no React, no network — because it is where the two mistakes
// that matter would live. A typed quantity or price is turned into an integer
// (milli-units, cents, basis points) by `money.ts` and never by arithmetic
// here, and a row that is not yet a line (no description, a price that is not
// a number) is reported as such rather than sent as a guess: the API replaces
// the *whole* set on every save, so one unusable row must stop the save, not
// quietly drop the line it stands for.
//
// A row is also a **snapshot** of a price-list item, exactly as the store
// models it: picking a product copies its description, unit, price and rate
// onto the row at that moment, and editing the price list afterwards never
// reaches back into a document.
import { hundredthsToInput, milliToInput, parseHundredths, parseMilli } from "./money";
import type { BillingProduct, DocumentLine, LineDraft } from "./types";

/** One row of the line grid, as text — what the inputs hold. */
export interface LineRow {
  /** Stable identity for React and for the remove action. Not sent: the API
   *  takes the set in order and assigns its own line ids. */
  key: string;
  description: string;
  unit: string;
  /** Quantity, as typed. Blank means one — the quantity a line is raised at
   *  when nobody says otherwise. */
  qty: string;
  /** Unit price, as typed. Blank means free. */
  price: string;
  /** VAT rate in percent, as typed. Blank means zero-rated. */
  rate: string;
  /**
   * The price-list item this line was picked from, when it was picked from one.
   *
   * Every document sends it; **only a quote has anywhere to put it**, and there
   * it decides what accepting the offer raises: an offer naming a stocked item
   * is for goods and becomes a sales order, one naming none becomes a draft
   * invoice. An invoice has no such column and its server drops the field. It
   * never prices anything — the description, unit, price and rate beside it are
   * the snapshot and stay the snapshot.
   *
   * Editing the description afterwards does not clear it: the line is still
   * that item, described in the seller's own words.
   */
  productId?: string | undefined;
}

/** What is wrong with a row, or `null` when it is a line. The order is the
 *  order the fields are read in, so a row reports its first problem. */
export type RowProblem = "description" | "qty" | "price" | "rate";

/** A row with nothing in it, ready to be typed into. */
export function blankRow(key: string): LineRow {
  return { key, description: "", unit: "", qty: "", price: "", rate: "" };
}

/** The stored line as an editable row. */
export function rowFromLine(line: DocumentLine): LineRow {
  return {
    key: line.id,
    description: line.description,
    unit: line.unit,
    qty: milliToInput(line.qtyMilli),
    price: hundredthsToInput(line.unitPriceCents),
    rate: hundredthsToInput(line.vatRateBp),
    // Carried back out of the stored line, so editing an offer and saving it
    // again does not quietly turn goods into a charge in words — which would
    // change what accepting it raises.
    productId: line.productId ?? undefined,
  };
}

/** A price-list item copied onto a row: description, unit, price and rate as
 *  they are **now**, which is what the document will carry for ever. The
 *  quantity is the row's own — picking an item does not decide how many. */
export function rowFromProduct(row: LineRow, product: BillingProduct): LineRow {
  return {
    key: row.key,
    description: product.name,
    unit: product.unit,
    qty: row.qty,
    price: hundredthsToInput(product.unitPriceCents),
    rate: hundredthsToInput(product.vatRateBp),
    // Which item was picked, kept beside the copy of its figures. On a quote
    // this is what makes an accepted offer become an order somebody can
    // deliver against; everywhere else it is carried and ignored.
    productId: product.id,
  };
}

/** Whether a row still holds nothing at all — a row that was added and never
 *  typed into, which is the one kind of unusable row that is simply dropped. */
export function isBlankRow(row: LineRow): boolean {
  return (
    row.description.trim() === "" &&
    row.unit.trim() === "" &&
    row.qty.trim() === "" &&
    row.price.trim() === "" &&
    row.rate.trim() === ""
  );
}

/** The first thing standing between this row and being a line, if anything. */
export function rowProblem(row: LineRow): RowProblem | null {
  if (row.description.trim() === "") return "description";
  if (row.qty.trim() !== "" && parseMilli(row.qty) === null) return "qty";
  if (row.price.trim() !== "" && parseHundredths(row.price) === null) return "price";
  if (row.rate.trim() !== "" && parseHundredths(row.rate) === null) return "rate";
  return null;
}

/** The line this row stands for, or `null` if it is not one yet. */
export function rowDraft(row: LineRow): LineDraft | null {
  if (rowProblem(row) !== null) return null;
  return {
    description: row.description.trim(),
    unit: row.unit.trim(),
    // A blank quantity is one, not none: a line nobody gave a number to bills
    // once. Blank money is zero, which is what the server defaults to as well.
    qtyMilli: row.qty.trim() === "" ? 1000 : (parseMilli(row.qty) ?? 0),
    unitPriceCents: row.price.trim() === "" ? 0 : (parseHundredths(row.price) ?? 0),
    vatRateBp: row.rate.trim() === "" ? 0 : (parseHundredths(row.rate) ?? 0),
    productId: row.productId,
  };
}

/**
 * The whole line set to send, or `null` when a row that is not blank cannot be
 * turned into a line. Wholly blank rows are dropped — clicking "add line" and
 * changing your mind is not an edit — and everything else must be a line,
 * because the API replaces the set in one write.
 */
export function rowsDraft(rows: LineRow[]): LineDraft[] | null {
  const drafts: LineDraft[] = [];
  for (const row of rows) {
    if (isBlankRow(row)) continue;
    const draft = rowDraft(row);
    if (draft === null) return null;
    drafts.push(draft);
  }
  return drafts;
}
