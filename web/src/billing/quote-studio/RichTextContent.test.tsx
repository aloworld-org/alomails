import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { RichTextContent } from "./RichTextContent";

describe("RichTextContent", () => {
  it("renders supported formatted content", () => {
    render(<RichTextContent value="<h2>Scope</h2><p>Details</p>" />);
    expect(screen.getByRole("heading", { name: "Scope" })).not.toBeNull();
    expect(screen.getByText("Details")).not.toBeNull();
  });
});
