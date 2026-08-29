import { describe, expect, it } from "vitest";

import {
  EMPTY_QUOTE_STUDIO_DESIGN,
  ensurePricingTable,
  normalizeSavedQuoteDesign,
} from "./quoteStudioNormalization";

describe("quote studio normalization", () => {
  it("keeps one pricing block available", () => {
    const design = ensurePricingTable({
      ...EMPTY_QUOTE_STUDIO_DESIGN,
      blocks: [],
    });
    expect(design.blocks).toEqual([
      { id: "pricing-table", kind: "pricing" },
    ]);
  });

  it("merges saved nested values with defaults", () => {
    const design = normalizeSavedQuoteDesign({
      colors: { accent: "#123456" } as never,
      headerDetails: { companyName: "Alo" } as never,
    });
    expect(design.colors.accent).toBe("#123456");
    expect(design.colors.text).toBe("#102a43");
    expect(design.headerDetails.companyName).toBe("Alo");
    expect(design.headerDetails.email).toBe("");
    expect(design.totalsStyle).toBe("soft");
  });

  it("preserves a saved totals presentation style", () => {
    const design = normalizeSavedQuoteDesign({ totalsStyle: "accent" });

    expect(design.totalsStyle).toBe("accent");
  });
});
