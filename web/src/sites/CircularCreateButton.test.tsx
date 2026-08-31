import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";

import { CircularCreateButton } from "./CircularCreateButton";

test("keeps a compact creation control accessible by name", () => {
  const onClick = vi.fn();
  render(<CircularCreateButton label="Create page" onClick={onClick} />);
  fireEvent.click(screen.getByRole("button", { name: "Create page" }));
  expect(onClick).toHaveBeenCalledOnce();
});
