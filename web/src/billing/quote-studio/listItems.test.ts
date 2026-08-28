import { describe, expect, it } from "vitest";

import {
  canIndentListItem,
  numberListItems,
  parseListItems,
  serializeListItems,
  shiftListItemLevel,
} from "./listItems";

describe("parseListItems / serializeListItems", () => {
  it("reads a design saved before nesting existed as top-level items", () => {
    expect(parseListItems("One\nTwo")).toEqual([
      { level: 0, text: "One" },
      { level: 0, text: "Two" },
    ]);
    expect(parseListItems("")).toEqual([]);
  });

  it("round-trips leading tabs as levels and clamps beyond the deepest", () => {
    const items = parseListItems("A\n\tB\n\t\tC\n\t\t\tD");
    expect(items.map((item) => item.level)).toEqual([0, 1, 2, 2]);
    expect(items[3]?.text).toBe("D");
    expect(serializeListItems(items)).toBe("A\n\tB\n\t\tC\n\t\tD");
  });
});

describe("shiftListItemLevel", () => {
  const items = parseListItems("A\n\tB\nC");

  it("nests an item at most one level under the item above", () => {
    // C sits under B (level 1), so it can go one level at a time to 2 —
    // and no further, because B is not nested deeper than that.
    const once = shiftListItemLevel(items, 2, 1);
    expect(once[2]?.level).toBe(1);
    expect(canIndentListItem(once, 2)).toBe(true);
    const twice = shiftListItemLevel(once, 2, 1);
    expect(twice[2]?.level).toBe(2);
    expect(canIndentListItem(twice, 2)).toBe(false);
    expect(shiftListItemLevel(twice, 2, 1)).toEqual(twice);
    // B sits under A (level 0): one level is the most it may nest.
    expect(canIndentListItem(items, 1)).toBe(false);
  });

  it("never nests the first item and never outdents past the top", () => {
    expect(shiftListItemLevel(items, 0, 1)).toEqual(items);
    expect(canIndentListItem(items, 0)).toBe(false);
    expect(shiftListItemLevel(items, 0, -1)).toEqual(items);
    expect(shiftListItemLevel(items, 2, -1)).toEqual(items);
    expect(shiftListItemLevel(items, 1, -1)[1]?.level).toBe(0);
  });
});

describe("numberListItems", () => {
  it("restarts a nested counter when a shallower item appears", () => {
    const markers = numberListItems(
      parseListItems("A\n\tB\n\tC\n\t\tD\nE\n\tF"),
      "decimal",
    ).map((item) => item.marker);
    expect(markers).toEqual(["1.", "a.", "b.", "i.", "2.", "a."]);
  });

  it("builds outline numbers from the ancestors' positions", () => {
    const markers = numberListItems(
      parseListItems("A\n\tB\n\tC\n\t\tD\nE"),
      "outline",
    ).map((item) => item.marker);
    expect(markers).toEqual(["1.", "1.1.", "1.2.", "1.2.1.", "2."]);
  });
});
