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
//
// Since D2.08 the rows are `ds/Table`'s. The one thing this table never had is
// the thing the component makes compulsory: a **name**. A board is a page of
// tables, and "table with 3 columns" nine times over is not a way to find the
// one you were reading — so the figure's own title names it, hidden on screen
// where the tile's heading already says it.
import { Table, Td, Th } from "../ds";
import { strings } from "../i18n";
import { align } from "./chart";
import type { Series } from "./types";

export function TableFigure({
  series,
  label,
  showLabel,
  scrollable,
}: {
  series: Series;
  /** What these figures are — the tile's title, or the question that was
   *  asked. Announced, and drawn only when `showLabel` says so. */
  label: string;
  showLabel?: boolean;
  /** `false` for the copy that sits behind a chart for a screen reader: it
   *  scrolls nothing, so it should not be a tab stop — and it takes no height
   *  ceiling either, since a ceiling on a box that cannot scroll would simply
   *  cut rows off. The scrolling form stops at 320px, which is where a long
   *  list starts scrolling inside the tile rather than pushing its footer off
   *  the board. */
  scrollable?: boolean;
}) {
  const model = align(series);
  return (
    <Table
      label={label}
      density="compact"
      stickyHeader
      flat
      {...(showLabel === true ? { showLabel: true } : {})}
      {...(scrollable === false
        ? { scrollable: false }
        : { className: "max-h-[320px]" })}
    >
      <thead>
        <tr>
          <Th>{strings.insightsColBucket}</Th>
          {model.series.map((group) => (
            <Th numeric key={group.key}>
              {model.multi ? group.name : strings.insightsColValue}
            </Th>
          ))}
        </tr>
      </thead>
      <tbody>
        {model.categories.map((category, index) => (
          <tr key={category}>
            <Th scope="row">{category}</Th>
            {model.series.map((group) => {
              const cell = group.values[index];
              return (
                <Td numeric key={group.key}>
                  {cell === undefined || cell.text === "" ? "—" : cell.text}
                </Td>
              );
            })}
          </tr>
        ))}
      </tbody>
    </Table>
  );
}
