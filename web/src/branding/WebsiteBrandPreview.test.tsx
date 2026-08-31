import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";

import { strings } from "../i18n";
import { DEFAULT_BRAND_KIT } from "./model";
import { WebsiteBrandPreview } from "./WebsiteBrandPreview";

test("website preview shows localized customer-facing copy", () => {
  render(<WebsiteBrandPreview kit={DEFAULT_BRAND_KIT} />);
  expect(screen.getByText(strings.brandingPreviewWebsiteHeading)).toBeTruthy();
  expect(screen.getByText(strings.brandingPreviewStartProject)).toBeTruthy();
});
