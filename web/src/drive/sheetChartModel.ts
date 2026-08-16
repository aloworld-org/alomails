// Turning a stored chart record into the neutral thing a chart draws
// (ADR 0051).
//
// The chart the sheet stores is ranges; what a renderer needs is figures. This
// file is the step between, and it is the browser's half of
// `platform/alo-ai/src/sheet_charts.rs` — deliberately the same rules, because
// a chart that reads one way on the server and another in the editor is worse
// than no chart.
//
// It knows nothing about ECharts. It produces `ChartModel`, the shape Insights
// already defined so that the engine stays a dependency rather than an
// architecture, and `EChart.tsx` remains the only file in the product that
// imports a chart library.
import type { ChartModel } from "../insights/chart";

import type { SheetChart, Snapshot } from "./sheetDocument";

/** Why a chart will not be drawn, as a code — the words a person reads are the
 *  client's, in their own language. Same three the Rust half reports. */
export type SheetChartError = "chartTabMissing" | "chartRangesRagged" | "chartTooLarge";

/** The most cells one chart may read. A chart of a hundred thousand points is
 *  not a chart, it is a way to freeze a browser. Matches `MAX_CHART_CELLS`. */
const MAX_CHART_CELLS = 5000;

interface Cell {
  row: number;
  col: number;
}

/** Reads `A2:B10`, or `B4` as the range of one. Normalised top-left first, so
 *  a selection dragged upwards is the same rectangle as one dragged down. */
export function parseRange(text: string): { start: Cell; end: Cell } | null {
  const parts = text.trim().split(":");
  const first = parseCell(parts[0] ?? "");
  const last = parseCell(parts[1] ?? parts[0] ?? "");
  if (first === null || last === null) return null;
  return {
    start: { row: Math.min(first.row, last.row), col: Math.min(first.col, last.col) },
    end: { row: Math.max(first.row, last.row), col: Math.max(first.col, last.col) },
  };
}

function parseCell(text: string): Cell | null {
  const match = /^\$?([A-Za-z]+)\$?(\d+)$/.exec(text.trim());
  if (match === null) return null;
  const letters = (match[1] ?? "").toUpperCase();
  const digits = match[2] ?? "";
  let col = 0;
  for (const character of letters) {
    col = col * 26 + (character.charCodeAt(0) - 64);
  }
  const row = Number.parseInt(digits, 10);
  if (!Number.isInteger(row) || row < 1) return null;
  return { row: row - 1, col: col - 1 };
}

/** The A1 form of a rectangle, which is what is stored and what a person reads. */
export function rangeReference(start: Cell, end: Cell): string {
  const one = (cell: Cell) => `${columnName(cell.col)}${cell.row + 1}`;
  return start.row === end.row && start.col === end.col
    ? one(start)
    : `${one(start)}:${one(end)}`;
}

function columnName(col: number): string {
  let name = "";
  let n = col + 1;
  while (n > 0) {
    const remainder = (n - 1) % 26;
    name = String.fromCharCode(65 + remainder) + name;
    n = Math.floor((n - 1) / 26);
  }
  return name;
}

function cellsOf(range: { start: Cell; end: Cell }): Cell[] {
  const out: Cell[] = [];
  for (let row = range.start.row; row <= range.end.row; row += 1) {
    for (let col = range.start.col; col <= range.end.col; col += 1) {
      out.push({ row, col });
    }
  }
  return out;
}

type StoredCell = { v?: unknown; t?: number };

function rawCell(workbook: Snapshot, tab: string, cell: Cell): StoredCell | undefined {
  const sheets = (workbook as { sheets?: Record<string, unknown> }).sheets;
  const sheet = sheets?.[tab] as { cellData?: Record<string, Record<string, StoredCell>> } | undefined;
  return sheet?.cellData?.[String(cell.row)]?.[String(cell.col)];
}

/** A cell's label — whatever the sheet shows there, blank if nothing. */
function labelAt(workbook: Snapshot, tab: string, cell: Cell): string {
  const value = rawCell(workbook, tab, cell)?.v;
  return value === undefined || value === null ? "" : String(value);
}

/**
 * A cell's figure, or `null` when the sheet does not hold one there.
 *
 * A number typed as text stays a gap. It is the commonest fault in a
 * spreadsheet, and a chart that quietly parsed the characters would draw a
 * total the grid itself does not agree with.
 */
function figureAt(workbook: Snapshot, tab: string, cell: Cell): number | null {
  const value = rawCell(workbook, tab, cell)?.v;
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

const NUMBER = new Intl.NumberFormat();

/**
 * Builds the drawable model for one stored chart, or the reason it cannot be
 * drawn.
 *
 * Every figure is the number the sheet holds, carried through untouched — this
 * function orders and pairs, and computes nothing.
 */
export function sheetChartModel(
  workbook: Snapshot,
  chart: SheetChart,
): { model: ChartModel } | { error: SheetChartError } {
  const categoryRange = parseRange(chart.categories);
  if (categoryRange === null) return { error: "chartRangesRagged" };
  const categoryCells = cellsOf(categoryRange);
  const parsed: { name: string; range: { start: Cell; end: Cell } }[] = [];
  for (const entry of chart.series) {
    const range = parseRange(entry.range);
    if (range === null) return { error: "chartRangesRagged" };
    parsed.push({ name: entry.name, range });
  }

  // Too large, then missing tab, then ragged — the same order as the Rust half,
  // and the order matters because a chart can fail two of these at once. One
  // that reported "ragged" here and "too large" on the server would send
  // somebody to fix the wrong thing. Size is first because a range of six
  // thousand cells is too large whether or not it also lines up.
  const budget =
    categoryCells.length +
    parsed.reduce((sum, entry) => sum + cellsOf(entry.range).length, 0);
  if (budget > MAX_CHART_CELLS) return { error: "chartTooLarge" };

  const sheets = (workbook as { sheets?: Record<string, unknown> }).sheets;
  if (sheets?.[chart.tab] === undefined) return { error: "chartTabMissing" };

  const seriesRanges: { name: string; cells: Cell[] }[] = [];
  for (const entry of parsed) {
    const cells = cellsOf(entry.range);
    // Paired by position, so a series must be as long as its labels — a chart
    // whose points and labels disagree draws every figure against the wrong
    // name, which is worse than drawing nothing.
    if (cells.length !== categoryCells.length) return { error: "chartRangesRagged" };
    seriesRanges.push({ name: entry.name, cells });
  }

  const categories = categoryCells.map((cell) => labelAt(workbook, chart.tab, cell));
  const series = seriesRanges.map((entry, index) => ({
    key: `s${index}`,
    name: entry.name,
    values: entry.cells.map((cell) => {
      const value = figureAt(workbook, chart.tab, cell);
      return { value, text: value === null ? "" : NUMBER.format(value) };
    }),
  }));

  return {
    model: {
      kind: chart.kind,
      categories,
      series,
      // A legend earns its space only when there is more than one thing to
      // tell apart.
      multi: series.length > 1,
      axisLabel: (value: number) => NUMBER.format(value),
    },
  };
}
