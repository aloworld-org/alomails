// The browser half of a chart's reading (ADR 0051), held to the same rules as
// the Rust half — because a chart that reads one way on the server and another
// in the editor is worse than no chart.
import { describe, expect, test } from "vitest";

import { parseRange, rangeReference, sheetChartModel } from "./sheetChartModel";
import type { SheetChart } from "./sheetDocument";

const WORKBOOK = {
  sheets: {
    s1: {
      name: "Sales",
      cellData: {
        "0": { "0": { v: "Month" }, "1": { v: "Revenue" } },
        "1": { "0": { v: "Jan" }, "1": { v: 120 } },
        "2": { "0": { v: "Feb" }, "1": { v: 90 } },
        "3": { "0": { v: "Mar" }, "1": { v: "n/a" } },
      },
    },
  },
};

const CHART: SheetChart = {
  id: "c1",
  title: "Revenue",
  kind: "bar",
  tab: "s1",
  categories: "A2:A4",
  series: [{ name: "Revenue", range: "B2:B4" }],
};

describe("reading a chart out of a sheet", () => {
  test("a range reads the same either way round", () => {
    // Dragging a selection upwards must not select nothing.
    expect(parseRange("C9:A2")).toEqual(parseRange("A2:C9"));
    expect(parseRange("B4")).toEqual({ start: { row: 3, col: 1 }, end: { row: 3, col: 1 } });
    expect(parseRange("nonsense")).toBeNull();
  });

  test("a reference survives a round trip, including past Z", () => {
    expect(rangeReference({ row: 1, col: 0 }, { row: 3, col: 0 })).toBe("A2:A4");
    expect(rangeReference({ row: 0, col: 27 }, { row: 0, col: 27 })).toBe("AB1");
  });

  test("the figures are the sheet's own, and a text cell is a gap", () => {
    const drawn = sheetChartModel(WORKBOOK, CHART);
    expect("model" in drawn).toBe(true);
    if (!("model" in drawn)) return;
    expect(drawn.model.categories).toEqual(["Jan", "Feb", "Mar"]);
    // "n/a" is text: a gap, never a zero nobody measured and never a parse of
    // the characters, which would draw a figure the grid does not hold.
    expect(drawn.model.series[0]?.values.map((v) => v.value)).toEqual([120, 90, null]);
    expect(drawn.model.multi).toBe(false);
  });

  test("a series that does not line up with its labels is refused", () => {
    const ragged = sheetChartModel(WORKBOOK, {
      ...CHART,
      series: [{ name: "Revenue", range: "B2:B3" }],
    });
    expect(ragged).toEqual({ error: "chartRangesRagged" });
  });

  test("a chart naming a deleted tab says which fault it is", () => {
    expect(sheetChartModel(WORKBOOK, { ...CHART, tab: "gone" })).toEqual({
      error: "chartTabMissing",
    });
  });

  test("a chart may not ask for the whole sheet", () => {
    expect(sheetChartModel(WORKBOOK, { ...CHART, categories: "A1:A6000" })).toEqual({
      error: "chartTooLarge",
    });
  });

  test("when a chart fails two ways at once, it fails the same way the server says", () => {
    // Too large AND naming a deleted tab. The Rust half reports size first, and
    // a client that disagreed would send somebody to fix the wrong thing.
    expect(
      sheetChartModel(WORKBOOK, { ...CHART, tab: "gone", categories: "A1:A6000" }),
    ).toEqual({ error: "chartTooLarge" });
  });

  test("two series earn a legend", () => {
    const drawn = sheetChartModel(WORKBOOK, {
      ...CHART,
      series: [
        { name: "Revenue", range: "B2:B4" },
        { name: "Also", range: "B2:B4" },
      ],
    });
    if (!("model" in drawn)) throw new Error("expected a model");
    expect(drawn.model.multi).toBe(true);
  });
});
