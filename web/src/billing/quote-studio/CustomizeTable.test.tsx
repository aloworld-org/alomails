import { describe, expect, it } from "vitest";

import { CustomizeTable } from "./CustomizeTable";

describe("CustomizeTable", () => {
  it("exports the pricing-table customization dialog", () => {
    expect(CustomizeTable).toBeTypeOf("function");
  });
});
