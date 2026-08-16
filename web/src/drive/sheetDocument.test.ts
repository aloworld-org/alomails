// The envelope's two obligations (ADR 0051): every sheet that exists today
// still opens, and a chart write never touches the grid.
import { describe, expect, test } from "vitest";

import { readSheetDocument, writeSheetDocument, type SheetChart } from "./sheetDocument";

const BARE = { name: "Q3", sheetOrder: ["s1"], sheets: { s1: { name: "Sales" } } };

const CHART: SheetChart = {
  id: "c1",
  title: "Revenue",
  kind: "bar",
  tab: "s1",
  categories: "A2:A4",
  series: [{ name: "Revenue", range: "B2:B4" }],
};

describe("the stored shape of an alo Sheet", () => {
  test("a bare snapshot is its own workbook — every sheet that exists today", () => {
    const read = readSheetDocument(BARE);
    expect(read.workbook).toEqual(BARE);
    expect(read.charts).toEqual([]);
  });

  test("an envelope round-trips and carries the grid through untouched", () => {
    const stored = writeSheetDocument({ workbook: BARE, charts: [CHART] });
    const read = readSheetDocument(stored);
    expect(read.workbook).toEqual(BARE);
    expect(read.charts).toEqual([CHART]);
  });

  test("wrapping an already-wrapped document does not nest it", () => {
    const once = writeSheetDocument({ workbook: BARE, charts: [CHART] });
    const twice = writeSheetDocument(readSheetDocument(once));
    expect(readSheetDocument(twice).workbook).toEqual(BARE);
    expect(readSheetDocument(twice).charts).toHaveLength(1);
  });

  test("a malformed chart is skipped rather than costing the workbook", () => {
    const stored = {
      schemaVersion: 1,
      workbook: BARE,
      charts: [CHART, { id: "bad", kind: "spiral", tab: "s1" }],
    };
    const read = readSheetDocument(stored);
    expect(read.charts).toEqual([CHART]);
    expect(read.workbook).toEqual(BARE);
  });

  test("an unknown kind is refused, not defaulted to a bar", () => {
    const stored = {
      workbook: BARE,
      charts: [{ ...CHART, kind: "scatter" }],
    };
    expect(readSheetDocument(stored).charts).toEqual([]);
  });

  test("a chart with no readable series is not a chart", () => {
    const stored = { workbook: BARE, charts: [{ ...CHART, series: [] }] };
    expect(readSheetDocument(stored).charts).toEqual([]);
  });

  test("nonsense reads as an empty document rather than throwing", () => {
    expect(readSheetDocument(null).workbook).toEqual({});
    expect(readSheetDocument("not a workbook").charts).toEqual([]);
  });
});
