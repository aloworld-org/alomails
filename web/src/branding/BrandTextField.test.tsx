import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";

import { BrandTextField } from "./BrandTextField";

test("brand text field connects its label, hint, and edit", () => {
  const onChange = vi.fn();
  render(<BrandTextField label="Purpose" hint="Why it exists" placeholder="Write purpose" value="" maximum={40} onChange={onChange} />);
  fireEvent.change(screen.getByLabelText("Purpose"), { target: { value: "Make work clearer" } });
  expect(screen.getByText("Why it exists")).toBeTruthy();
  expect(onChange).toHaveBeenCalledWith("Make work clearer");
});
