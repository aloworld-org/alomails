import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";

import { BrandMark } from "./BrandMark";
import { DEFAULT_BRAND_KIT } from "./model";

test("brand mark derives a fallback monogram from the brand name", () => {
  render(<BrandMark kit={{ ...DEFAULT_BRAND_KIT, foundation: { ...DEFAULT_BRAND_KIT.foundation, name: "Northstar" } }} />);
  expect(screen.getByText("N")).toBeTruthy();
});
