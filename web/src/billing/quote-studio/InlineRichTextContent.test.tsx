import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { InlineRichTextContent } from "./InlineRichTextContent";

describe("InlineRichTextContent", () => {
  it("renders inline formatting without unsupported links", () => {
    render(<InlineRichTextContent value='<strong>Item</strong><a href="#"> link</a>' />);
    expect(screen.getByText("Item").tagName).toBe("STRONG");
    expect(screen.queryByRole("link")).toBeNull();
  });
});
