// What ten hand-rolled tables did not do.
//
// A table's appearance is the easy half and the copies had already agreed on
// it. These are the properties that were missing everywhere — each one is
// invisible until somebody reads the screen with something other than their
// eyes, or scrolls it with something other than a mouse.
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, test } from "vitest";

import { Table, TableEmpty, Td, Th } from "./Table";

afterEach(cleanup);

function list(rows: string[] = ["Kaufmann GmbH"]) {
  return render(
    <Table label="Customers">
      <thead>
        <tr>
          <Th>Name</Th>
          <Th numeric>Outstanding</Th>
          <Th hideLabel>Actions</Th>
        </tr>
      </thead>
      <tbody>
        {rows.length === 0 ? (
          <TableEmpty cols={3}>No customers yet</TableEmpty>
        ) : (
          rows.map((name) => (
            <tr key={name}>
              <Td>{name}</Td>
              <Td numeric>1 240,00</Td>
              <Td>
                <button>Archive</button>
              </Td>
            </tr>
          ))
        )}
      </tbody>
    </Table>,
  );
}

describe("the table behaves for somebody not using a mouse and eyes", () => {
  test("it names itself", () => {
    list();
    // Without this a screen reader announces "table, 3 columns" and leaves
    // the reader to guess what is in it. All ten copies left it to guess.
    expect(screen.getByRole("table", { name: "Customers" })).toBeDefined();
  });

  test("the name is read but not drawn, unless asked for", () => {
    const { unmount } = list();
    // Hidden from the eye and never from the accessibility tree: `display:
    // none` here would remove the table's name, which is the opposite of the
    // point. The caption is the `<caption>` either way — the second assertion
    // is what says so.
    expect(screen.getByText("Customers").className).toContain("sr-only");
    expect(screen.getByRole("table", { name: "Customers" })).toBeDefined();
    unmount();

    render(
      <Table label="Customers" showLabel>
        <tbody>
          <tr>
            <Td>Kaufmann GmbH</Td>
          </tr>
        </tbody>
      </Table>,
    );
    expect(screen.getByText("Customers").className).not.toContain("sr-only");
    expect(screen.getByRole("table", { name: "Customers" })).toBeDefined();
  });

  test("the scrolling region can be reached by keyboard", () => {
    list();
    // `overflow: auto` on a plain div is scrollable by mouse and unreachable
    // by keyboard (WCAG 2.1.1). The region is the tab stop, and it is named
    // so that the stop is explicable.
    const region = screen.getByRole("region", { name: "Customers" });
    expect(region.tabIndex).toBe(0);
  });

  test("a column header is associated with its column", () => {
    list();
    expect(
      screen.getByRole("columnheader", { name: "Name" }).getAttribute("scope"),
    ).toBe("col");
  });

  test("a header with no visible text still has a name", () => {
    list();
    // The actions column. Drawn empty, announced as "Actions" — a column with
    // no header at all is announced as nothing, which is worse than a label
    // the sighted reader does not need.
    const actions = screen.getByRole("columnheader", { name: "Actions" });
    expect(actions.textContent).toBe("Actions");
    expect(actions.firstElementChild?.className).toContain("sr-only");
  });

  test("empty means a row saying so, not an empty grid", () => {
    list([]);
    const cell = screen.getByRole("cell", { name: "No customers yet" });
    // Spanning every column: a message in the first column of an otherwise
    // empty row reads as a value for that column.
    expect(cell.getAttribute("colspan")).toBe("3");
  });

  test("amounts line up", () => {
    list();
    // Two halves, and the second is the one a hand-rolled table forgot: right
    // alignment puts the units under the units, and tabular figures make every
    // digit the same width, without which they still do not line up.
    const cell = screen.getByRole("cell", { name: "1 240,00" });
    expect(cell.className).toContain("text-right");
    expect(cell.className).toContain("tabular-nums");
    // And no other cell in the row claims either.
    const name = screen.getByRole("cell", { name: "Kaufmann GmbH" });
    expect(name.className).not.toContain("text-right");
    expect(name.className).not.toContain("tabular-nums");
  });

  test("and so does the heading above them", () => {
    list();
    // The heading of a numeric column sits over its figures, not over the
    // column to its left. It has to say so at a weight the base header rule
    // cannot outrank — `.table th { text-align: left }` beat a plain
    // `.numeric` on specificity, so this was wrong everywhere until D1.55 —
    // and the marker that carries that weight is the attribute.
    const heading = screen.getByRole("columnheader", { name: "Outstanding" });
    expect(heading.getAttribute("data-align")).toBe("end");
    expect(heading.className).toContain("[&[data-align]]:text-right");
    // A header nobody aligned is still left, and carries no marker at all.
    const plain = screen.getByRole("columnheader", { name: "Name" });
    expect(plain.getAttribute("data-align")).toBe(null);
  });
});
