import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { TableToggle } from "./TableToggle";

describe("TableToggle", () => {
  it("reports and changes its selected state", () => {
    const onClick = vi.fn();
    render(
      <TableToggle
        label="Unit"
        help="Show the unit column"
        checked
        onClick={onClick}
      />,
    );

    const toggle = screen.getByRole("button", { name: /unit/i });
    expect(toggle.getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(toggle);
    expect(onClick).toHaveBeenCalledOnce();
  });
});
