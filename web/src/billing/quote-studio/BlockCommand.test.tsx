import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { BlockCommand } from "./BlockCommand";

describe("BlockCommand", () => {
  it("exposes the command label and invokes it", () => {
    const onClick = vi.fn();
    render(<BlockCommand label="Move" onClick={onClick}>↑</BlockCommand>);
    fireEvent.click(screen.getByRole("button", { name: "Move" }));
    expect(onClick).toHaveBeenCalledOnce();
  });
});
