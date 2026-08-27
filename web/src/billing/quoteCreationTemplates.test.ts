import { describe, expect, test } from "vitest";

import { strings } from "../i18n";
import { quoteCreationTemplates } from "./quoteCreationTemplates";
import type { BillingProduct } from "./types";

function product(name: string, unit: string): BillingProduct {
  return {
    id: name,
    name,
    unit,
    unitPriceCents: 12500,
    vatRateBp: 2100,
    sku: "",
    barcode: "",
    stocked: false,
    purchasePriceCents: 0,
    photoNodeId: null,
    defaultSupplierId: null,
    archived: false,
    archivedAt: null,
    createdBy: "test",
    createdAt: "2026-08-27T00:00:00Z",
    updatedAt: "2026-08-27T00:00:00Z",
  };
}

describe("quoteCreationTemplates", () => {
  test("builds distinct blank, services, proposal, and retainer starts", () => {
    const templates = quoteCreationTemplates([
      product("Workshop", "day"),
      product("Support", "hour"),
      product("Hosting", "month"),
    ]);
    let key = 0;
    const nextKey = () => `row-${++key}`;

    expect(templates.map(({ name }) => name)).toEqual([
      strings.billingQuoteTemplateBlank,
      strings.billingQuoteTemplateServices,
      strings.billingQuoteTemplateProject,
      strings.billingQuoteTemplateRetainer,
    ]);
    expect(templates[0]?.buildRows(nextKey)).toEqual([]);
    expect(templates[1]?.buildRows(nextKey)).toHaveLength(2);
    expect(templates[2]?.buildRows(nextKey)).toHaveLength(3);
    expect(templates[3]?.buildRows(nextKey)[0]?.description).toBe("Hosting");
  });

  test("excludes archived and stocked products from service templates", () => {
    const archived = { ...product("Old", "hour"), archived: true };
    const stocked = { ...product("Part", "piece"), stocked: true };
    const templates = quoteCreationTemplates([archived, stocked]);

    expect(templates[1]?.buildRows(() => "row")).toEqual([]);
  });
});
