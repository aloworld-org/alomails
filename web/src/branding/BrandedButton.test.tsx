import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";

import { BrandedButton } from "./BrandedButton";

test("branded button exposes preview copy without pretending to be interactive", () => {
  render(<BrandedButton>Continue</BrandedButton>);
  expect(screen.queryByRole("button")).toBeNull();
  expect(screen.getByText("Continue")).toBeTruthy();
});
