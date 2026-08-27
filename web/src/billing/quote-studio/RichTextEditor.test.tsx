import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RichTextEditor } from "./RichTextEditor";

describe("RichTextEditor", () => {
  it("emits edited content", () => {
    const onChange = vi.fn();
    render(<RichTextEditor value="" label="Body" placeholder="Write" onChange={onChange} />);
    const editor = screen.getByRole("textbox", { name: "Body" });
    editor.innerHTML = "New copy";
    fireEvent.input(editor);
    expect(onChange).toHaveBeenLastCalledWith("New copy");
  });
});
