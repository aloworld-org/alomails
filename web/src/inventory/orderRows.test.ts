// The order row model, tested where it is pure — the two places a warehouse
// screen can lose a number without anyone noticing.
import { describe, expect, test } from "vitest";

import type { BillingProduct } from "../billing";
import {
  blankOrderRow,
  fulfilDraft,
  orderRowFromLine,
  orderRowFromProduct,
  orderRowsDraft,
} from "./orderRows";
import type { OrderLine } from "./types";

const PRODUCT: BillingProduct = {
  id: "p-chair",
  name: "Blue chair",
  unit: "piece",
  unitPriceCents: 9_900,
  vatRateBp: 1900,
  sku: "CH-1",
  barcode: "",
  stocked: true,
  purchasePriceCents: 6_000,
  defaultSupplierId: null,
  photoNodeId: null,
  archived: false,
  archivedAt: null,
  createdBy: "u-1",
  createdAt: "2026-08-01T10:00:00Z",
  updatedAt: "2026-08-01T10:00:00Z",
};

const LINE: OrderLine = {
  id: "line-1",
  productId: "p-chair",
  description: "Blue chair",
  unit: "piece",
  qtyMilli: 2_500,
  unitPriceCents: 6_000,
  vatRateBp: 1900,
  netCents: 15_000,
};

describe("a line's catalog link", () => {
  test("a picked product is copied at the price of the direction it is going", () => {
    const row = blankOrderRow("r-1");
    expect(orderRowFromProduct(row, PRODUCT, "purchase").price).toBe("60");
    expect(orderRowFromProduct(row, PRODUCT, "sale").price).toBe("99");
    // The link itself, both ways: it is what a receipt turns into goods.
    expect(orderRowFromProduct(row, PRODUCT, "purchase").productId).toBe("p-chair");
  });

  test("a stored line comes back as the row that would send it again", () => {
    const row = orderRowFromLine(LINE);
    expect(row).toMatchObject({ productId: "p-chair", qty: "2.5", price: "60", rate: "19" });
    expect(orderRowsDraft([row])).toEqual([
      {
        productId: "p-chair",
        description: "Blue chair",
        unit: "piece",
        qtyMilli: 2_500,
        unitPriceCents: 6_000,
        vatRateBp: 1900,
      },
    ]);
  });

  test("a line typed by hand carries no link, and moves nothing", () => {
    const freight = { ...blankOrderRow("r-2"), description: "Freight", price: "25" };
    expect(orderRowsDraft([freight])).toEqual([
      {
        productId: "",
        description: "Freight",
        unit: "",
        // Nobody said how many, so it is one — the same default the server has.
        qtyMilli: 1_000,
        unitPriceCents: 2_500,
        vatRateBp: 0,
      },
    ]);
  });

  test("an untouched row is dropped, and an unusable one stops the whole save", () => {
    const typed = { ...blankOrderRow("r-3"), description: "Blue chair" };
    expect(orderRowsDraft([typed, blankOrderRow("r-4")])).toHaveLength(1);
    // The API replaces the set in one write, so a row that cannot become a line
    // must stop the save rather than quietly leave the order.
    expect(orderRowsDraft([typed, { ...blankOrderRow("r-5"), qty: "two" }])).toBeNull();
  });
});

describe("a consignment sheet", () => {
  test("states only the lines somebody put a quantity on", () => {
    expect(
      fulfilDraft([
        { lineId: "a", qty: "4" },
        { lineId: "b", qty: "" },
        // Typed down to zero is the same as left alone: asking the server to
        // record a movement of no goods is not what anybody meant.
        { lineId: "c", qty: "0" },
      ]),
    ).toEqual([{ lineId: "a", qtyMilli: 4_000 }]);
  });

  test("a quantity that is not one refuses the whole sheet", () => {
    expect(fulfilDraft([{ lineId: "a", qty: "four" }])).toBeNull();
  });

  test("a blank sheet is an answer, and a different one from a refused sheet", () => {
    expect(fulfilDraft([{ lineId: "a", qty: "" }])).toEqual([]);
  });
});
