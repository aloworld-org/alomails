import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { InlineRichTextEditor } from "./InlineRichTextEditor";

describe("InlineRichTextEditor", () => {
  it("emits sanitized inline content", () => {
    const onChange = vi.fn();
    render(<InlineRichTextEditor value="" placeholder="Write" aria-label="Item" onChange={onChange} />);
    const editor = screen.getByRole("textbox", { name: "Item" });
    editor.innerHTML = '<strong class="bad">Safe</strong><a href="#"> link</a>';
    fireEvent.input(editor);
    expect(onChange).toHaveBeenLastCalledWith("<strong>Safe</strong> link");
  });
});
