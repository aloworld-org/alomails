import { render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";

import { strings } from "../i18n";
import { DEFAULT_BRAND_KIT } from "./model";
import { VisualIdentityView } from "./VisualIdentityView";

test("visual identity groups logo, color, and typography roles", () => {
  render(<VisualIdentityView kit={DEFAULT_BRAND_KIT} onChange={vi.fn()} />);
  expect(screen.getByRole("heading", { name: strings.brandingLogoTitle })).toBeTruthy();
  expect(screen.getByRole("heading", { name: strings.brandingColorsTitle })).toBeTruthy();
  expect(screen.getByRole("heading", { name: strings.brandingTypographyTitle })).toBeTruthy();
});
