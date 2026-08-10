// The line model decides what leaves the browser, so its two dangerous
// answers are pinned here: which typed row becomes which integers, and when
// the whole set must not be sent at all. Everything else on the editor is
// arrangement; this is where a wrong invoice would be written.
import { describe, expect, it } from "vitest";

import {
  blankRow,
  isBlankRow,
  rowDraft,
  rowFromLine,
  rowFromProduct,
  rowProblem,
  rowsDraft,
} from "./lineRows";
import type { LineRow } from "./lineRows";
import type { BillingProduct, DocumentLine } from "./types";

const LINE: DocumentLine = {
  id: "l-1",
  description: "Consulting",
  unit: "hour",
  qtyMilli: 1500,
  unitPriceCents: 12500,
  vatRateBp: 2100,
  netCents: 18750,
};

const PRODUCT: BillingProduct = {
  id: "p-1",
  name: "Consulting",
  unit: "hour",
  unitPriceCents: 12500,
  vatRateBp: 2100,
  // The catalog half (B5.02/B5.03): a service, so nothing about it is stocked.
  sku: "",
  barcode: "",
  stocked: false,
  purchasePriceCents: 0,
  photoNodeId: null,
  defaultSupplierId: null,
  archived: false,
  archivedAt: null,
  createdBy: "u-1",
  createdAt: "2026-08-06T10:00:00Z",
  updatedAt: "2026-08-06T10:00:00Z",
};

function row(fields: Partial<LineRow>): LineRow {
  return { ...blankRow("k"), ...fields };
}

describe("a row", () => {
  it("round-trips a stored line without changing a single figure", () => {
    const back = rowDraft(rowFromLine(LINE));
    expect(back).toEqual({
      description: "Consulting",
      unit: "hour",
      qtyMilli: 1500,
      unitPriceCents: 12500,
      vatRateBp: 2100,
    });
  });

  it("takes a price-list item as a snapshot, keeping the quantity typed", () => {
    const picked = rowFromProduct(row({ qty: "3", description: "something else" }), PRODUCT);
    expect(picked.description).toBe("Consulting");
    expect(picked.price).toBe("125");
    expect(picked.rate).toBe("21");
    expect(picked.qty).toBe("3");
  });

  it("bills once when nobody says how many, and is free when nobody says a price", () => {
    expect(rowDraft(row({ description: "Site visit" }))).toEqual({
      description: "Site visit",
      unit: "",
      qtyMilli: 1000,
      unitPriceCents: 0,
      vatRateBp: 0,
    });
  });

  it("reports its first problem, in the order the fields are read", () => {
    expect(rowProblem(row({ description: " " }))).toBe("description");
    expect(rowProblem(row({ description: "X", qty: "two" }))).toBe("qty");
    expect(rowProblem(row({ description: "X", price: "twelve fifty" }))).toBe("price");
    expect(rowProblem(row({ description: "X", rate: "high" }))).toBe("rate");
    expect(rowProblem(row({ description: "X", qty: "1.5", price: "1 234,56", rate: "5,5" }))).toBeNull();
  });

  it("is blank until something is typed into it", () => {
    expect(isBlankRow(blankRow("k"))).toBe(true);
    expect(isBlankRow(row({ price: "10" }))).toBe(false);
  });
});

describe("a line set", () => {
  it("drops the untouched rows and keeps the order of the rest", () => {
    const lines = rowsDraft([
      { ...rowFromLine(LINE), key: "a" },
      blankRow("b"),
      row({ key: "c", description: "Discount", qty: "-1", price: "50" }),
    ]);
    expect(lines?.map((l) => l.description)).toEqual(["Consulting", "Discount"]);
    expect(lines?.[1]?.qtyMilli).toBe(-1000);
  });

  it("refuses to be sent at all while one row is not a line", () => {
    // The API replaces the whole set in one write, so a set that quietly left
    // the offending row out would delete it from the document.
    expect(rowsDraft([{ ...rowFromLine(LINE), key: "a" }, row({ key: "b", price: "50" })])).toBeNull();
    expect(rowsDraft([row({ key: "a", description: "X", qty: "1.2345" })])).toBeNull();
  });

  it("is empty rather than null when there is nothing on the document", () => {
    expect(rowsDraft([])).toEqual([]);
    expect(rowsDraft([blankRow("a")])).toEqual([]);
  });
});
