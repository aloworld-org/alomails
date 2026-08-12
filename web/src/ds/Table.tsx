// The one data table (ADR 0045).
//
// Ten stylesheets declared `.table`, and five of them — crm, finance, hr,
// inventory, projects — were byte-identical down to the `10px 14px` padding.
// The three that genuinely differed differed in ways worth keeping: billing
// wants a header that stays put while a long list scrolls under it, insights
// wants a dense table inside a card, and both want a footer row of totals.
// Those are `stickyHeader` and `density`; everything else was an accident.
//
// The styling was never the hard part. What every one of the ten was missing:
//
//   * A scroll container a keyboard can reach. All ten set `overflow: auto`
//     on a plain `<div>`, which is a region a mouse can scroll and a keyboard
//     cannot (WCAG 2.1.1). `Table` renders that container as a labelled,
//     focusable region.
//   * A name. A screen reader announces "table with 7 columns" and nothing
//     else unless the table says what it lists. `label` is required, and
//     becomes a `<caption>` — visible only if you ask for it, but always read.
//   * An empty state inside the table. A list that renders its "no matches"
//     line *beside* the table leaves a screen-reader user in an empty grid
//     with no explanation.
//
// The base rules target `th`/`td` directly, so plain markup inside `<Table>`
// is already styled correctly and a migration is a deletion. `Th` and `Td`
// exist only for the things a class name was carrying: alignment, numerals,
// and a column header that is present for a screen reader but not on screen.
import type { ReactNode, TdHTMLAttributes, ThHTMLAttributes } from "react";

import styles from "./Table.module.css";

/** Where the text in a cell sits. `numeric` is the common right-hand case and
 *  carries tabular figures with it, so a column of amounts lines up. */
export type CellAlign = "start" | "center" | "end";

export interface TableProps {
  /** What this table lists — "Customers", "Invoice lines". Required: it is
   *  the table's accessible name, and it is the one thing every hand-rolled
   *  table in this codebase left out. */
  label: string;
  /** Show the label above the table. Off by default: most screens already
   *  carry a heading, and repeating it is noise on screen but never noise to
   *  a screen reader. */
  showLabel?: boolean | undefined;
  /** `compact` for a table inside a card or a panel, where the rows are a
   *  supporting detail rather than the screen's subject. */
  density?: "compact" | "default" | undefined;
  /** Keep the header visible while the body scrolls. Only meaningful when the
   *  region actually scrolls — a long list in a fixed-height pane. */
  stickyHeader?: boolean | undefined;
  /** Highlight the row under the pointer. Opt in, and only when a row really
   *  responds to a click: a hover state on inert content is a promise the
   *  screen does not keep. */
  interactiveRows?: boolean | undefined;
  /** Drop the border, radius and background of the scroll region, for a table
   *  that sits directly on a `Card` that already draws them. */
  flat?: boolean | undefined;
  /** `<thead>`, `<tbody>`, `<tfoot>` — ordinary table markup. */
  children: ReactNode;
  /** Applied to the scroll region, which is the element that lays out. */
  className?: string | undefined;
}

export function Table({
  label,
  showLabel,
  density,
  stickyHeader,
  interactiveRows,
  flat,
  children,
  className,
}: TableProps) {
  const region = [
    styles.region,
    flat === true ? styles.flat : "",
    className ?? "",
  ]
    .filter(Boolean)
    .join(" ");
  const table = [
    styles.table,
    density === "compact" ? styles.compact : "",
    stickyHeader === true ? styles.sticky : "",
    interactiveRows === true ? styles.interactive : "",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    // The scroll container is a `<div>` because `overflow` on a `<table>` is
    // not honoured — which is why all ten copies wrapped one too. `tabIndex`
    // goes here rather than on the table: a region that scrolls has to be
    // reachable by keyboard, and giving it a role and a name is what stops
    // that tab stop from being an unexplained one.
    <div className={region} tabIndex={0} role="region" aria-label={label}>
      <table className={table}>
        <caption className={showLabel === true ? styles.caption : styles.srOnly}>
          {label}
        </caption>
        {children}
      </table>
    </div>
  );
}

// `align` shadows the presentational HTML attribute of the same name, which
// has been deprecated since HTML 4 and which nothing here should be setting.
export interface ThProps
  extends Omit<ThHTMLAttributes<HTMLTableCellElement>, "align"> {
  align?: CellAlign | undefined;
  /** Right-aligned with tabular figures, for a column of amounts. */
  numeric?: boolean | undefined;
  /** The header exists for a screen reader but is not drawn — an actions
   *  column, a column of row checkboxes. The text still has to be there:
   *  a nameless column is announced as nothing at all. */
  hideLabel?: boolean | undefined;
}

/** A column header. `scope="col"` unless told otherwise, because an
 *  unscoped header does not associate with anything. */
export function Th({
  align,
  numeric,
  hideLabel,
  scope = "col",
  className,
  children,
  ...rest
}: ThProps) {
  return (
    <th scope={scope} className={cell(align, numeric, className)} {...rest}>
      {hideLabel === true ? (
        <span className={styles.srOnly}>{children}</span>
      ) : (
        children
      )}
    </th>
  );
}

export interface TdProps
  extends Omit<TdHTMLAttributes<HTMLTableCellElement>, "align"> {
  align?: CellAlign | undefined;
  /** Right-aligned with tabular figures, for a column of amounts. */
  numeric?: boolean | undefined;
}

export function Td({ align, numeric, className, ...rest }: TdProps) {
  return <td className={cell(align, numeric, className)} {...rest} />;
}

export interface TableEmptyProps {
  /** How many columns the table has, so the message spans all of them. */
  cols: number;
  children: ReactNode;
}

/** The "nothing here" row, inside `<tbody>`. Inside the table rather than
 *  beside it: a table whose explanation lives in a sibling paragraph reads,
 *  to anyone navigating by table, as a table with no rows and no reason. */
export function TableEmpty({ cols, children }: TableEmptyProps) {
  return (
    <tr>
      <td className={styles.empty} colSpan={cols}>
        {children}
      </td>
    </tr>
  );
}

function cell(
  align: CellAlign | undefined,
  numeric: boolean | undefined,
  className: string | undefined,
): string | undefined {
  const classes = [
    numeric === true ? styles.numeric : "",
    align === "center" ? styles.center : "",
    align === "end" ? styles.end : "",
    align === "start" ? styles.start : "",
    className ?? "",
  ]
    .filter(Boolean)
    .join(" ");
  return classes === "" ? undefined : classes;
}
