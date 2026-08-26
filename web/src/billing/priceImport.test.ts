import { describe, expect, test } from "vitest";

import { importNumber, readPriceImport, suggestColumn } from "./priceImport";

describe("price-list import", () => {
  test("reads quoted CSV rows and preserves commas inside names", async () => {
    const file = new File([
      'Product,Unit,Unit price,VAT rate,SKU\n"Steel plate, 4 mm",piece,"1.250,50",21%,ST-4\n',
    ], "prices.csv", { type: "text/csv" });
    const result = await readPriceImport(file);
    expect(result.headers).toEqual(["Product", "Unit", "Unit price", "VAT rate", "SKU"]);
    expect(result.rows[0]).toEqual(["Steel plate, 4 mm", "piece", "1.250,50", "21%", "ST-4"]);
  });

  test("suggests familiar supplier column names", () => {
    const headers = ["Item code", "Description", "UOM", "Sales price", "Tax rate"];
    expect(suggestColumn(headers, "sku")).toBe(0);
    expect(suggestColumn(headers, "name")).toBe(1);
    expect(suggestColumn(headers, "unit")).toBe(2);
    expect(suggestColumn(headers, "unitPrice")).toBe(3);
    expect(suggestColumn(headers, "vat")).toBe(4);
  });

  test("understands European and international price formatting", () => {
    expect(importNumber("€ 1.250,50")).toBe(1250.5);
    expect(importNumber("1,250.50")).toBe(1250.5);
    expect(importNumber("21%")).toBe(21);
    expect(importNumber("not a price")).toBeNull();
  });
});
