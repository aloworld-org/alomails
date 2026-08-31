import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";

import { strings } from "../i18n";
import { BrandGuidelinesView } from "./BrandGuidelinesView";
import { DEFAULT_BRAND_KIT } from "./model";

test("guidelines make incomplete foundation fields explicit", () => {
  render(<BrandGuidelinesView kit={DEFAULT_BRAND_KIT} />);
  expect(screen.getAllByText(strings.brandingGuidelineMissing).length).toBeGreaterThan(0);
  expect(screen.getByText(strings.brandingGuidelineLogoMissing)).toBeTruthy();
});
