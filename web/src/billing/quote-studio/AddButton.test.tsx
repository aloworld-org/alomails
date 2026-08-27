import { fireEvent, render, screen } from "@testing-library/react";
import { Plus } from "lucide-react";
import { describe, expect, it, vi } from "vitest";

import { AddButton } from "./AddButton";

describe("AddButton", () => {
  it("runs its action and presents its help", () => {
    const onClick = vi.fn();
    render(<AddButton label="Block" help="Block help" Icon={Plus} onClick={onClick} />);
    fireEvent.click(screen.getByRole("button", { name: /block/i }));
    expect(onClick).toHaveBeenCalledOnce();
    expect(screen.getByText("Block help")).toBeTruthy();
  });
});
