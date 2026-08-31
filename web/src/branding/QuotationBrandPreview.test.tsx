import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";

import { strings } from "../i18n";
import { DEFAULT_BRAND_KIT } from "./model";
import { QuotationBrandPreview } from "./QuotationBrandPreview";

test("quotation preview applies the shared brand to a financial document", () => {
  render(<QuotationBrandPreview kit={DEFAULT_BRAND_KIT} />);
  expect(screen.getByText(strings.brandingPreviewQuoteLabel)).toBeTruthy();
  expect(screen.getByText("€8,850")).toBeTruthy();
});
