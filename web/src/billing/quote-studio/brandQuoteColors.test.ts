import { expect, test } from "vitest";

import { DEFAULT_BRAND_KIT } from "../../branding/model";
import { DEFAULT_QUOTE_COLORS } from "./QuoteStudioDesign";
import { importBrandQuoteColors } from "./brandQuoteColors";

test("brand colors map into quotation roles without overwriting page surfaces", () => {
  const colors = importBrandQuoteColors(DEFAULT_BRAND_KIT, DEFAULT_QUOTE_COLORS);
  expect(colors.accent).toBe(DEFAULT_BRAND_KIT.primary.value);
  expect(colors.text).toBe(DEFAULT_BRAND_KIT.secondary?.value);
  expect(colors.background).toBe(DEFAULT_QUOTE_COLORS.background);
  expect(colors.headerBackground).toBe(DEFAULT_QUOTE_COLORS.headerBackground);
});
