import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";

import { strings } from "../i18n";
import { DEFAULT_BRAND_KIT } from "./model";
import { PaletteInsights } from "./PaletteInsights";

test("palette insights show generated tones and contrast guidance", () => {
  render(<PaletteInsights kit={DEFAULT_BRAND_KIT} />);
  expect(screen.getByText(strings.brandingWcagAa)).toBeTruthy();
  expect(screen.getByText(strings.brandingBalanceRatio)).toBeTruthy();
});
