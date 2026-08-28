import { render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { strings } from "../../i18n";
import { GeneralTableBlock } from "./GeneralTableBlock";

describe("GeneralTableBlock", () => {
  it("renders a localized empty editor", () => {
    render(<GeneralTableBlock block={{ id: "table", kind: "table", columns: [{ id: "one", label: "Name" }], rows: [] }} readOnly={false} onChange={vi.fn()} />);
    expect(screen.getByRole("button", { name: strings.quoteStudioAddRowBelow })).toBeTruthy();
  });

  it("places column and row tools above their editable text", () => {
    const { container } = render(
      <GeneralTableBlock
        block={{
          id: "table",
          kind: "table",
          columns: [{ id: "one", label: "Name" }, { id: "two", label: "Value" }],
          rows: [{ id: "row-one", cells: { one: "Project", two: "Ficina" } }],
        }}
        readOnly={false}
        onChange={vi.fn()}
      />,
    );
    const table = within(container);
    const removeColumn = table.getByRole("button", {
      name: strings.quoteStudioRemoveColumnA11y("Name"),
    });
    const columnEditor = table.getByRole("textbox", {
      name: strings.quoteStudioColumnNameA11y(1),
    });
    const removeRow = table.getByRole("button", {
      name: strings.quoteStudioRemoveRowA11y(1),
    });
    const cellEditor = table.getByRole("textbox", {
      name: strings.quoteStudioTableCellA11y("Name", 1),
    });
    expect(removeColumn.compareDocumentPosition(columnEditor) & Node.DOCUMENT_POSITION_FOLLOWING).not.toBe(0);
    expect(removeRow.compareDocumentPosition(cellEditor) & Node.DOCUMENT_POSITION_FOLLOWING).not.toBe(0);
  });
});
