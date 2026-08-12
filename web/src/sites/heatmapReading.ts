// Turning a heatmap grid into something a person can read (S2.09b). Pure, so
// every rule here is pinned by a test rather than inspected on a screen.
//
// Three rules live here, and they are the reason the module exists apart from
// the view:
//
//  1. **A map is suppressed below a minimum sample.** The store returns
//     whatever it collected, including a page with two clicks (S2.09a flagged
//     this as the UI's job). Two clicks drawn as a heatmap is a picture of two
//     people, presented with the authority of a thousand.
//  2. **Every colour has words beside it.** The overlay is aggregated into
//     named regions — a third of the width by a tenth of the height — so the
//     same finding is reachable by a screen reader and by anyone who does not
//     separate the colours.
//  3. **Positions are described, never counted in cells.** "Centre, 30–40%
//     down" is what an owner can act on; "column 17, row 22" is an
//     implementation detail of the collection grid.
import { strings } from "../i18n";
import type { AnalyticsRow } from "./AnalyticsPanels";
import type { SiteHeatmapCell, SiteHeatmapScrollBucket } from "./types";

/** How many events one screen class needs before its map is drawn at all.
 *  Twenty is the point where a shape stops being three individuals; it is a
 *  presentation threshold, deliberately not a rule the store enforces. */
export const HEATMAP_MINIMUM_SAMPLE = 20;

/** How many horizontal bands the written summary uses. Three, because "left,
 *  centre, right" is the vocabulary people already have for a page. */
const SIDES = 3;

/** How many vertical bands it uses — tenths, matching the depth curve so the
 *  two panels describe the page the same way. */
const DEPTH_BANDS = 10;

/** Whether a total is too small to be shown as a map. */
export function tooFewToShow(total: number): boolean {
  return total < HEATMAP_MINIMUM_SAMPLE;
}

/** Which third of the width a column falls in, in words. Uses the column's
 *  centre, so an edge column is never rounded into the neighbouring third. */
export function sideLabel(column: number, columns: number): string {
  if (columns <= 0) return strings.sitesHeatmapCentre;
  const across = (column + 0.5) / columns;
  if (across < 1 / SIDES) return strings.sitesHeatmapLeft;
  if (across < 2 / SIDES) return strings.sitesHeatmapCentre;
  return strings.sitesHeatmapRight;
}

/** One tenth of the page, named by the share of the page it covers. */
export function depthBandLabel(band: number): string {
  return strings.sitesHeatmapDepthBand(band * 10, band * 10 + 10);
}

/** Which tenth of the page a grid row falls in, clamped so a grid that grows
 *  a row cannot produce an eleventh band. */
function depthBand(row: number, rows: number): number {
  if (rows <= 0) return 0;
  const down = Math.floor(((row + 0.5) / rows) * DEPTH_BANDS);
  return Math.min(Math.max(down, 0), DEPTH_BANDS - 1);
}

/** The click grid as a ranked list of named regions, busiest first. Cells are
 *  summed into regions rather than listed one by one: a single cell is 1/32 of
 *  the width and means nothing on its own, and a list of two thousand
 *  coordinates is not a summary. */
export function clickRegions(
  cells: SiteHeatmapCell[],
  columns: number,
  rows: number,
): AnalyticsRow[] {
  const totals = new Map<string, number>();
  for (const cell of cells) {
    const label = strings.sitesHeatmapSpot(
      sideLabel(cell.column, columns),
      depthBandLabel(depthBand(cell.row, rows)),
    );
    totals.set(label, (totals.get(label) ?? 0) + cell.hits);
  }
  return [...totals]
    .map(([label, visits]) => ({ label, visits }))
    .sort((a, b) => b.visits - a.visits || a.label.localeCompare(b.label));
}

/** The depth curve as named rows in depth order, all ten kept — including the
 *  tenths nobody reached, which are the interesting ones. */
export function depthRows(buckets: SiteHeatmapScrollBucket[]): AnalyticsRow[] {
  return [...buckets]
    .sort((a, b) => a.bucket - b.bucket)
    .map((bucket) => ({
      label: depthBandLabel(bucket.bucket),
      visits: bucket.hits,
    }));
}

/** How strongly one cell is painted, `0`–`1`. The busiest cell is full
 *  strength and every other is its share of it, lifted off the floor so a
 *  single click is still visible rather than invisibly faint. */
export function cellIntensity(hits: number, busiest: number): number {
  if (busiest <= 0) return 0;
  return 0.18 + 0.82 * Math.min(hits / busiest, 1);
}
