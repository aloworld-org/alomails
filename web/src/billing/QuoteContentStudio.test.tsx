import { describe, expect, it } from "vitest";

import {
  DEFAULT_QUOTE_COLUMNS,
  QuoteContentStudio,
  saveQuoteTemplateDesign,
} from "./QuoteContentStudio";

describe("QuoteContentStudio public API", () => {
  it("exposes the studio component and its focused public utilities", () => {
    expect(QuoteContentStudio).toBeDefined();
    expect(saveQuoteTemplateDesign).toBeTypeOf("function");
    expect(DEFAULT_QUOTE_COLUMNS.net).toBe(true);
  });
});
