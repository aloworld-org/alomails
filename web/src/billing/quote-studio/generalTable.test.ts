import { describe, expect, it } from "vitest";
import type { GeneralTable } from "./QuoteStudioBlock";
import { generalTableHasContent } from "./generalTable";

const table = (value: string): GeneralTable => ({
  id: "table-1",
  kind: "table",
  columns: [{ id: "column-1", label: "Column" }],
  rows: [{ id: "row-1", cells: { "column-1": value } }],
});

describe("generalTableHasContent", () => {
  it("ignores whitespace-only cells", () => {
    expect(generalTableHasContent(table("  "))).toBe(false);
    expect(generalTableHasContent(table("Value"))).toBe(true);
  });
});
