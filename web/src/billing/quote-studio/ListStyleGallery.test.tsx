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
    const onChange = vi.fn();
    render(
      <div data-testid="clipped-editor" className="overflow-hidden">
        <ListStyleGallery ordered={false} value="disc" onChange={onChange} />
      </div>,
    );
    const trigger = screen.getByRole("button", { name: strings.quoteStudioBulletStyle });
    expect(trigger.textContent).not.toContain(strings.quoteStudioListStyleName("disc"));
    fireEvent.click(trigger);
    const dialog = screen.getByRole("dialog", { name: strings.quoteStudioBulletStyle });
    expect(screen.getAllByRole("radio")).toHaveLength(7);
    expect(screen.getByTestId("clipped-editor").contains(dialog)).toBe(false);
    expect(dialog.parentElement?.className).toContain("fixed");
    expect(dialog.className).toContain("overflow-y-auto");

    const checkbox = screen.getByRole("radio", {
      name: strings.quoteStudioListStyleName("checkbox"),
    });
    expect(checkbox.textContent).toContain("☐");
    expect(checkbox.getAttribute("title")).toBeNull();

    const selected = screen.getByRole("radio", {
      name: strings.quoteStudioListStyleName("disc"),
    });
    const selectedMark = selected.querySelector("svg")?.parentElement;
    expect(selectedMark?.className).toContain("right-2.5");
    expect(selectedMark?.className).toContain("top-2.5");

    fireEvent.keyDown(selected, { key: "Tab", shiftKey: true });
    expect(screen.getAllByRole("radio").at(-1)).toBe(document.activeElement);

    fireEvent.click(checkbox);
    expect(onChange).toHaveBeenCalledWith("checkbox");
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
