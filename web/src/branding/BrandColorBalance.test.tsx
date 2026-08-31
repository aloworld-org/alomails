import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";

import { strings } from "../i18n";
import { BrandColorBalance } from "./BrandColorBalance";

test("color balance presents the brand roles and their recommended proportions", () => {
  render(<BrandColorBalance primary="#E76F51" secondary="#102A43" />);

  expect(screen.getByLabelText(strings.brandingColorBalance)).toBeTruthy();
  expect(screen.getByText(strings.brandingNeutral)).toBeTruthy();
  expect(screen.getByText("70%")).toBeTruthy();
  expect(screen.getByText("20%")).toBeTruthy();
  expect(screen.getByText("10%")).toBeTruthy();
});
