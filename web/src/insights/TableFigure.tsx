// The same answer as rows (ADR 0037, wave BI1.05).
//
// A table is the `table` viz, and it is also what a screen reader is given for
// every chart on a board: a canvas has no rows, so the figures are put in the
// document as well as on the pixels. One component serves both, which is the
// only way the two can be guaranteed to say the same thing.
//
// It lists the buckets the server returned, in the order it returned them, and
// shows each group's own formatted figure. A bucket a group had no answer for
// is an em dash, not a zero: a gap is not a measurement.
import { strings } from "../i18n";
import { align } from "./chart";
import type { Series } from "./types";
import styles from "./InsightsModule.module.css";

export function TableFigure({ series, caption }: { series: Series; caption?: string }) {
  const model = align(series);
  return (
    <div className={styles.tableWrap}>
      <table className={styles.table}>
        {caption !== undefined && <caption className={styles.tableCaption}>{caption}</caption>}
        <thead>
          <tr>
            <th scope="col">{strings.insightsColBucket}</th>
            {model.series.map((group) => (
              <th scope="col" key={group.key} className={styles.numeric}>
                {model.multi ? group.name : strings.insightsColValue}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {model.categories.map((category, index) => (
            <tr key={category}>
              <th scope="row">{category}</th>
              {model.series.map((group) => {
                const cell = group.values[index];
                return (
                  <td key={group.key} className={styles.numeric}>
                    {cell === undefined || cell.text === "" ? "—" : cell.text}
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
