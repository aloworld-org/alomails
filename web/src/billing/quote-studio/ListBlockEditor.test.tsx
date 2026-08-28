import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { strings } from "../../i18n";
import { ListBlockEditor } from "./ListBlockEditor";

afterEach(cleanup);

describe("ListBlockEditor", () => {
  it("renders each list item", () => {
    render(
      <ListBlockEditor ordered={false} items={"One\nTwo"} columns={1} style="disc" onChange={vi.fn()} />,
    );
    expect(screen.getByText("One")).not.toBeNull();
    expect(screen.getByText("Two")).not.toBeNull();
  });

  it("indents an item by writing a leading tab into the stored items", () => {
    const onChange = vi.fn();
    render(
      <ListBlockEditor ordered items={"One\nTwo"} columns={1} style="decimal" onChange={onChange} />,
    );
    const indent = screen.getByRole("button", { name: strings.quoteStudioIndentItem });
    expect((indent as HTMLButtonElement).disabled).toBe(true);
    fireEvent.focus(screen.getByRole("textbox", {
      name: strings.quoteStudioNumberedItemA11y(2),
    }));
    expect((indent as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(indent);
    expect(onChange).toHaveBeenCalledWith({ items: "One\n\tTwo" });
  });

  it("shows the marker of the chosen style on each row", () => {
    render(
      <ListBlockEditor ordered items={"One\n\tTwo"} columns={1} style="parenthesis" onChange={vi.fn()} />,
    );
    expect(screen.getByText("1)")).not.toBeNull();
    expect(screen.getByText("a)")).not.toBeNull();
  });

  it("places the active-item tools beside the list guidance", () => {
    render(
      <ListBlockEditor ordered={false} items="One" columns={1} style="disc" onChange={vi.fn()} />,
    );
    const toolbar = screen.getByRole("toolbar", {
      name: strings.quoteStudioListItemFormatting,
    });
    const guidance = screen.getByText(strings.quoteStudioListLayoutHelp);
    expect(guidance.parentElement?.contains(toolbar)).toBe(true);
  });
});
