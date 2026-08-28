import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { strings } from "../../i18n";
import { ListStyleGallery } from "./ListStyleGallery";

afterEach(cleanup);

describe("ListStyleGallery", () => {
  it("offers the numbering library for a numbered list and reports the choice", () => {
    const onChange = vi.fn();
    render(<ListStyleGallery ordered value="decimal" onChange={onChange} />);
    fireEvent.click(screen.getByRole("button", { name: strings.quoteStudioNumberingStyle }));
    const tiles = screen.getAllByRole("radio");
    expect(tiles).toHaveLength(6);
    expect(screen.getByRole("radio", { name: strings.quoteStudioListStyleName("decimal") }).getAttribute("aria-checked")).toBe("true");
    // The outline tile is drawn with the real numbering.
    const outline = screen.getByRole("radio", { name: strings.quoteStudioListStyleName("outline") });
    expect(outline.textContent).toContain("1.2.1.");
    fireEvent.click(outline);
    expect(onChange).toHaveBeenCalledWith("outline");
    expect(screen.queryByRole("radiogroup")).toBeNull();
  });

  it("offers the bullet library for a bullet list", () => {
    render(<ListStyleGallery ordered={false} value="disc" onChange={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: strings.quoteStudioBulletStyle }));
    expect(screen.getAllByRole("radio")).toHaveLength(7);
    expect(screen.getByRole("radio", { name: strings.quoteStudioListStyleName("checkbox") }).textContent).toContain("☐");
  });
});
