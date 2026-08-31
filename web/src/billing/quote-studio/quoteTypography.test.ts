import { expect, test } from "vitest";

import { DEFAULT_BRAND_KIT } from "../../branding/model";
import { importBrandQuoteTypography, themeQuoteTypography } from "./quoteTypography";

test("quotation typography can use a preset or exact workspace brand roles", () => {
  expect(themeQuoteTypography("editorial")).toEqual({ headingFont: "georgia", bodyFont: "inter" });
  expect(importBrandQuoteTypography({ ...DEFAULT_BRAND_KIT, typography: { heading: "garamond", body: "arial" } })).toEqual({ headingFont: "garamond", bodyFont: "arial" });
});
