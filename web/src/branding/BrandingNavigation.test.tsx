import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { expect, test } from "vitest";

import { strings } from "../i18n";
import { BrandingNavigation } from "./BrandingNavigation";

test("branding navigation exposes four named destinations", () => {
  render(<MemoryRouter initialEntries={["/branding/foundation"]}><BrandingNavigation /></MemoryRouter>);
  const nav = screen.getByRole("navigation", { name: strings.brandingNavLabel });
  expect(nav.querySelectorAll("a")).toHaveLength(4);
  expect(screen.getByRole("link", { name: strings.brandingFoundationNav }).getAttribute("href")).toBe("/branding/foundation");
});
