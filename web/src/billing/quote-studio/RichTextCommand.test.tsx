import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RichTextCommand } from "./RichTextCommand";

describe("RichTextCommand", () => {
  it("exposes its label and invokes its command", () => {
    const onClick = vi.fn();
    render(<RichTextCommand label="Bold" onClick={onClick}>B</RichTextCommand>);
    fireEvent.click(screen.getByRole("button", { name: "Bold" }));
    expect(onClick).toHaveBeenCalledOnce();
  });
});
