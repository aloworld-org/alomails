// A single figure, big enough to read across a room (ADR 0037, wave BI1.05).
//
// The `number` viz is the one a business looks at first — what is owed, what
// was billed this quarter — so it is the plainest thing on a board: the
// server's figure, formatted, and a caption only when there is something the
// figure does not say on its own.
//
// It draws one figure per group, which is normally one. There is a second when
// money could not honestly be restated into a single currency
// (`docs/design/insights.md` § Deals are never converted): two figures, each
// with its own currency, is the truth — a single total would not be.
import { figureText, pointLabel } from "./format";
import type { Series } from "./types";
import { TOTAL_BUCKET } from "./format";
import styles from "./InsightsModule.module.css";

export function NumberFigure({ series }: { series: Series }) {
  const many = series.series.length > 1;
  return (
    <div className={styles.numbers}>
      {series.series.flatMap((group) =>
        group.points.map((point) => {
          // The caption says which figure this is when there is more than one:
          // the currency it is in, or the bucket it belongs to. A single total
          // needs neither — the tile's own title already said it.
          const caption =
            point.bucket === TOTAL_BUCKET
              ? many
                ? group.key
                : null
              : pointLabel(point);
          return (
            <p className={styles.number} key={`${group.key}:${point.bucket}`}>
              <span className={styles.numberValue}>{figureText(series, group, point.value)}</span>
              {caption !== null && <span className={styles.numberCaption}>{caption}</span>}
            </p>
          );
        }),
      )}
    </div>
  );
}
