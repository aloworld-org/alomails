import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";

import { PaletteInsightCard } from "./PaletteInsightCard";

test("palette insight card presents a named finding and its evidence", () => {
  render(<PaletteInsightCard title="Contrast" meta="AA"><span>Passes</span></PaletteInsightCard>);
  expect(screen.getByRole("heading", { name: "Contrast" })).toBeTruthy();
  expect(screen.getByText("Passes")).toBeTruthy();
});
