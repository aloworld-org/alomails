import { describe, it, expect } from "vitest";

import { univerSnapshotToXlsx } from "./exportOffice";
import { xlsxToUniverSnapshot } from "./importOffice";

// A Univer-shaped snapshot: A1 string, B1 number, A2 boolean, on two sheets.
const SNAPSHOT = {
  name: "Budget",
  sheetOrder: ["s1", "s2"],
  sheets: {
    s1: {
      name: "Q1",
      cellData: {
        0: { 0: { v: "Revenue", t: 1 }, 1: { v: 1250.5, t: 2 } },
        1: { 0: { v: true, t: 3 } },
      },
    },
    s2: {
      name: "Notes",
      cellData: { 0: { 0: { v: "Draft <ok> & \"done\"", t: 1 } } },
    },
  },
};

describe("univerSnapshotToXlsx", () => {
  it("round-trips values, types, sheet names, and order through the importer", () => {
    const bytes = univerSnapshotToXlsx(SNAPSHOT);
    const back = xlsxToUniverSnapshot(bytes, "Budget");

    const order = back.sheetOrder as string[];
    const sheets = back.sheets as Record<
      string,
      { name: string; cellData: Record<number, Record<number, { v: unknown; t: number }>> }
    >;
    const first = sheets[order[0] as string];
    const second = sheets[order[1] as string];

    expect(first?.name).toBe("Q1");
    expect(second?.name).toBe("Notes");
    expect(first?.cellData[0]?.[0]).toEqual({ v: "Revenue", t: 1 });
    expect(first?.cellData[0]?.[1]).toEqual({ v: 1250.5, t: 2 });
    expect(first?.cellData[1]?.[0]).toEqual({ v: true, t: 3 });
  });

  it("escapes XML-special characters in string cells", () => {
    const bytes = univerSnapshotToXlsx(SNAPSHOT);
    const back = xlsxToUniverSnapshot(bytes, "Budget");
    const order = back.sheetOrder as string[];
    const sheets = back.sheets as Record<
      string,
      { cellData: Record<number, Record<number, { v: unknown }>> }
    >;
    const notes = sheets[order[1] as string];
    expect(notes?.cellData[0]?.[0]?.v).toBe('Draft <ok> & "done"');
  });

  it("produces a valid one-sheet workbook from an empty snapshot", () => {
    const bytes = univerSnapshotToXlsx({});
    const back = xlsxToUniverSnapshot(bytes, "Empty");
    expect((back.sheetOrder as string[]).length).toBe(1);
  });
});
