// The envelope an alo Sheet is stored in (ADR 0051), and the one place either
// shape of a stored blob is understood.
//
// The grid engine regenerates its snapshot from its own state on every save, so
// anything alo writes *into* that object is gone by the next keystroke. That is
// why a chart is stored beside the snapshot rather than inside it, and why this
// file exists: the editor unwraps on load and wraps on save, and nothing else
// in the product has to know there are two shapes.
//
// The Rust half of this contract is `platform/alo-ai/src/sheet_charts.rs`, which
// reads the same envelope for the Sheet agent. The two are deliberately small
// and deliberately identical in their tolerance: an unreadable chart is skipped,
// never fatal, because one bad record must not cost somebody the workbook it
// was in.

/** A grid-engine workbook snapshot — opaque to alo, persisted verbatim. */
export type Snapshot = Record<string, unknown>;

/** The kinds `EChart.tsx` can draw. A kind is a promise of a renderer, not a
 *  wish: anything else is refused on read rather than defaulted to a bar. */
export type SheetChartKind = "bar" | "line" | "pie";

export interface SheetChartSeries {
  name: string;
  /** An A1 range, e.g. `B2:B10`. Ranges, never values — a chart that stored
   *  figures could disagree with the cells it came from. */
  range: string;
}

export interface SheetChart {
  id: string;
  title: string;
  kind: SheetChartKind;
  /** The tab key in the snapshot's `sheets` object, so renaming a tab does not
   *  orphan its charts. */
  tab: string;
  categories: string;
  series: SheetChartSeries[];
}

export interface SheetDocument {
  workbook: Snapshot;
  charts: SheetChart[];
}

const SCHEMA_VERSION = 1;

function isKind(value: unknown): value is SheetChartKind {
  return value === "bar" || value === "line" || value === "pie";
}

function readChart(raw: unknown): SheetChart | null {
  if (typeof raw !== "object" || raw === null) return null;
  const record = raw as Record<string, unknown>;
  const { id, title, kind, tab, categories, series } = record;
  if (typeof id !== "string" || typeof tab !== "string") return null;
  if (typeof categories !== "string" || !isKind(kind)) return null;
  if (!Array.isArray(series)) return null;
  const read = series.flatMap((entry): SheetChartSeries[] => {
    if (typeof entry !== "object" || entry === null) return [];
    const item = entry as Record<string, unknown>;
    if (typeof item.range !== "string") return [];
    return [{ name: typeof item.name === "string" ? item.name : "", range: item.range }];
  });
  // A chart with nothing readable to draw is a record of an intention, not a
  // chart; keeping it would put an empty frame on the sheet.
  if (read.length === 0) return null;
  return {
    id,
    title: typeof title === "string" ? title : "",
    kind,
    tab,
    categories,
    series: read,
  };
}

/**
 * Reads a stored blob, whichever shape it is in.
 *
 * A blob with a `workbook` key is an envelope. Anything else is the older bare
 * snapshot — which is every sheet that exists today — and is its own workbook
 * with no charts. Those open unchanged and gain an envelope the first time they
 * are saved.
 */
export function readSheetDocument(raw: unknown): SheetDocument {
  if (typeof raw !== "object" || raw === null) {
    return { workbook: {}, charts: [] };
  }
  const record = raw as Record<string, unknown>;
  const inner = record.workbook;
  if (typeof inner !== "object" || inner === null) {
    return { workbook: record as Snapshot, charts: [] };
  }
  const charts = Array.isArray(record.charts)
    ? record.charts.flatMap((entry) => {
        const chart = readChart(entry);
        return chart === null ? [] : [chart];
      })
    : [];
  return { workbook: inner as Snapshot, charts };
}

/**
 * Puts a blob back together for storage — always the current envelope, so a
 * sheet saved by this product is in one shape whatever it arrived as.
 *
 * The workbook half is carried through untouched. A save path that edited a
 * cell on its way past would be the one bug nobody would think to look for
 * here.
 */
export function writeSheetDocument(document: SheetDocument): Record<string, unknown> {
  return {
    schemaVersion: SCHEMA_VERSION,
    workbook: document.workbook,
    charts: document.charts,
  };
}
