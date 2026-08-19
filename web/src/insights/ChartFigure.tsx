// A bar, a line or a pie (ADR 0037, wave BI1.05) — the drawn vizzes, each one
// the same answer the table shows.
//
// Two rules give this component its shape, and both are the design note's:
//
//   - **A pie is one whole.** When money comes back in more than one currency
//     the answer has one group per currency, and slices of euros beside slices
//     of dollars would be a picture of nothing. So a pie is drawn once per
//     group; a bar and a line take every group at once, told apart by the
//     legend, which is what the note means by "one series per currency".
//   - **A canvas is not a document.** Every chart is accompanied by the same
//     figures as a table, hidden from sight and present for a screen reader —
//     so the numbers are readable by everyone, not just by whoever can see the
//     pixels.
import { Suspense, useMemo } from "react";

import { Spinner } from "../ds";
import { Chart, chartModel } from "./chart";
import { TableFigure } from "./TableFigure";
import type { Series } from "./types";
import styles from "./InsightsModule.module.css";

/** The figures behind the pixels: present in the document, not on the screen.
 *  Tailwind's own `sr-only`, which is the same declaration this module's
 *  `.srOnly` was (D2.08). */
const SCREEN_READER_ONLY = "sr-only";

/** The vizzes this component draws — `number` and `table` are their own. */
export type DrawnViz = "bar" | "line" | "pie";

export function ChartFigure({
  series,
  viz,
  title,
}: {
  series: Series;
  viz: DrawnViz;
  title: string;
}) {
  // One pie per group, one cartesian chart for all of them.
  const models = useMemo(
    () =>
      viz === "pie"
        ? series.series.map((group) => ({
            key: group.key,
            model: chartModel(series, viz, group.key),
          }))
        : [{ key: "all", model: chartModel(series, viz) }],
    [series, viz],
  );

  return (
    <div className={styles.figure}>
      <div className={models.length > 1 ? styles.chartsSplit : styles.charts}>
        {models.map(({ key, model }) => (
          <Suspense key={key} fallback={<Spinner size={18} />}>
            <Chart
              model={model}
              label={models.length > 1 ? `${title} — ${key}` : title}
            />
          </Suspense>
        ))}
      </div>
      <div className={SCREEN_READER_ONLY}>
        <TableFigure series={series} label={title} scrollable={false} />
      </div>
    </div>
  );
}
