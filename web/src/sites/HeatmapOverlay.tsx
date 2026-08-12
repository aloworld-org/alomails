// The picture half of the attention map (S2.09b). Presentational only: it is
// handed cells that already passed the minimum-sample gate and draws them.
//
// It draws a **proportional page**, not a screenshot and not a phone-height
// viewport: the grid S2.09a collects spans the whole scrollable page, so the
// frame keeps the grid's own aspect ratio and is labelled top and bottom. An
// overlay drawn over one screenful would put the same click in a different
// place on every screen — which is exactly what the shared grid avoids.
//
// The picture is `aria-hidden` behind one honest label; the readable version
// of the same data is the written region list beside it, never a promise that
// a screen reader can interpret coloured squares.
import type { CSSProperties } from "react";

import { strings } from "../i18n";
import { cellIntensity } from "./heatmapReading";
import type { SiteHeatmapCell } from "./types";
import styles from "./SitesModule.module.css";

export function HeatmapOverlay({
  cells,
  columns,
  rows,
  label,
}: {
  cells: SiteHeatmapCell[];
  columns: number;
  rows: number;
  /** The accessible name: what page, what screen class, how many clicks. */
  label: string;
}) {
  const busiest = cells.reduce((max, cell) => Math.max(max, cell.hits), 0);

  return (
    <div className={styles.heatmapFigure}>
      <p className={styles.heatmapAxis}>{strings.sitesHeatmapTop}</p>
      <div
        className={styles.heatmapFrame}
        role="img"
        aria-label={label}
        style={{ "--heatmap-aspect": `${columns} / ${rows}` } as CSSProperties}
      >
        {cells.map((cell) => (
          <span
            key={`${cell.column}:${cell.row}`}
            className={styles.heatmapCell}
            aria-hidden="true"
            style={
              {
                "--heatmap-x": cell.column / columns,
                "--heatmap-y": cell.row / rows,
                "--heatmap-w": 1 / columns,
                "--heatmap-h": 1 / rows,
                "--heatmap-value": cellIntensity(cell.hits, busiest),
              } as CSSProperties
            }
          />
        ))}
      </div>
      <p className={styles.heatmapAxis}>{strings.sitesHeatmapBottom}</p>
      <p className={styles.heatmapLegend} aria-hidden="true">
        <span>{strings.sitesHeatmapLegendQuiet}</span>
        <span className={styles.heatmapLegendBar} />
        <span>{strings.sitesHeatmapLegendBusy}</span>
      </p>
    </div>
  );
}
