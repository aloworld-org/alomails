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
//
// Styled with Tailwind utilities generated from `ds/tokens.css` (ADR 0046), so
// `--space-4` and `p-4` are one definition with two spellings. That restyle
// changed no rule; the D1.55 wave check then changed exactly one — a numeric
// column's header is now right-aligned over its figures. See `TH_ALIGN`.
import type { ReactNode, TdHTMLAttributes, ThHTMLAttributes } from "react";

import { cx } from "./cx";

/** The scroll container itself. Its border, radius and ground are chosen once
 *  below rather than layered with a `flat` override: two utilities setting the
 *  same property have no defined winner, because Tailwind emits them in its
 *  own order rather than in the order they appear in `class`. */
const REGION =
  "overflow-auto " +
  "focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-offset-2";

/** What `flat` drops, for a table sitting directly on a `Card` that already
 *  draws all three. Omitting them is the whole of `flat`: a `<div>` has no
 *  border, no radius and no background of its own. */
const SURFACE = "rounded-lg border border-subtle bg-surface";

const TABLE = "w-full border-collapse text-sm";

/** The cell rules reach `th` and `td` *through* the table, exactly as the
 *  stylesheet's descendant selectors did. That is what makes ordinary markup
 *  inside `<Table>` already right and every migration a deletion — and it is
 *  also why the two cell components below cannot simply layer over these: a
 *  descendant utility is one class and one element, which outranks a plain
 *  utility on the cell itself. */
const CELLS =
  "[&_th]:text-left [&_th]:text-tertiary [&_th]:font-medium [&_th]:whitespace-nowrap " +
  "[&_td]:text-primary [&_td]:align-middle";

/** Padding and borders together, because `grid` changes both and a data
 *  table's rules and a grid's would otherwise be two utilities fighting over
 *  one property with nothing to decide between them. `default` and `compact`
 *  are the two densities; `grid` replaces the pair. */
const RULED = {
  default:
    "[&_th]:px-4 [&_th]:py-3 [&_td]:px-4 [&_td]:py-3 " +
    "[&_th]:border-b [&_th]:border-subtle [&_td]:border-b [&_td]:border-subtle " +
    // The last row's separator would draw against the container's own border.
    "[&_tbody_tr:last-child_td]:border-b-0",
  compact:
    "[&_th]:px-3 [&_th]:py-2 [&_td]:px-3 [&_td]:py-2 " +
    "[&_th]:border-b [&_th]:border-subtle [&_td]:border-b [&_td]:border-subtle " +
    "[&_tbody_tr:last-child_td]:border-b-0",
} as const;

/** A table you type into. Every cell bounded on all four sides, because a value
 *  you are editing has to show where it ends and the next begins; and no cell
 *  padding, because each cell is filled edge to edge by the control that edits
 *  it. The outer edge is drawn by the cells themselves, so the last row keeps
 *  the bottom border a data table drops. */
const GRID =
  "[&_th]:p-0 [&_td]:p-0 " +
  "[&_th]:border [&_th]:border-subtle [&_td]:border [&_td]:border-subtle";

/** Totals. billing's footer sat on `--bg-raised` and read as part of the
 *  table's structure rather than as another row, which is right. Two elements
 *  and a class, so it outranks the base cell rules above. */
const FOOT =
  "[&_tfoot]:bg-raised " +
  "[&_tfoot_th]:border-t [&_tfoot_th]:border-default [&_tfoot_th]:border-b-0 [&_tfoot_th]:text-primary " +
  "[&_tfoot_td]:border-t [&_tfoot_td]:border-default [&_tfoot_td]:border-b-0 [&_tfoot_td]:text-primary";

/** A header that survives scrolling needs its own background, or the rows
 *  scroll through it. */
const STICKY =
  "[&_thead_th]:sticky [&_thead_th]:top-0 [&_thead_th]:z-1 [&_thead_th]:bg-surface";

const ROW_HOVER = "[&_tbody_tr:hover]:bg-raised";

const CAPTION = "pb-2 text-left text-xs text-tertiary";

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
  /** The table does not scroll in a box of its own: whatever holds it decides
   *  its size, and it takes its full height there.
   *
   *  This is not styling — it removes the tab stop. The wrapper is a keyboard
   *  stop *because* it scrolls (see the region below), and a table that cannot
   *  scroll is a stop that goes nowhere and shows nothing. The caller that
   *  found it is `insights/ChartFigure`, whose every chart carries the same
   *  figures as a table for a screen reader: nine charts on a board would have
   *  been nine invisible tab stops in `sr-only` containers, which is worse for
   *  a sighted keyboard user than the chart alone. The `<caption>` still names
   *  the table, so nothing is lost to a screen reader.
   *
   *  Added for `insights/ChartFigure` (D2.08). */
  scrollable?: boolean | undefined;
  /** A table that is typed into rather than read: every cell bounded on all
   *  four sides, and no cell padding, because each cell is filled edge to edge
   *  by the control that edits it. The Docs table block is the caller — an
   *  editable table still wants the name, the caption and the reachable scroll
   *  region a data table gets, and only its cells work differently. */
  grid?: boolean | undefined;
  /** `<thead>`, `<tbody>`, `<tfoot>` — ordinary table markup. */
  children: ReactNode;
  /** Applied to the scroll region, which is the element that lays out. */
  className?: string | undefined;
}

export function Table({
  label,
  showLabel,
  density = "default",
  stickyHeader,
  interactiveRows,
  flat,
  grid,
  scrollable = true,
  children,
  className,
}: TableProps) {
  const region = [
    scrollable ? REGION : "",
    flat === true ? "" : SURFACE,
    className ?? "",
  ]
    .filter(Boolean)
    .join(" ");
  const table = [
    TABLE,
    CELLS,
    grid === true ? GRID : RULED[density],
    FOOT,
    stickyHeader === true ? STICKY : "",
    interactiveRows === true ? ROW_HOVER : "",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    // The scroll container is a `<div>` because `overflow` on a `<table>` is
    // not honoured — which is why all ten copies wrapped one too. `tabIndex`
    // goes here rather than on the table: a region that scrolls has to be
    // reachable by keyboard, and giving it a role and a name is what stops
    // that tab stop from being an unexplained one.
    //
    // Without `scrollable` the three go together: no overflow, so no tab stop,
    // so no role and no name to explain one. A `<div>` carrying `aria-label`
    // and no role names nothing anyway; the `<caption>` is what names the
    // table, and it is there in both forms.
    <div
      className={region}
      {...(scrollable
        ? {
            tabIndex: 0,
            role: "region",
            "aria-label": label,
            // The region scrolls on purpose; the responsive e2e sweep exempts
            // marked containers from its element-width invariant.
            "data-allow-overflow": "",
          }
        : {})}
    >
      <table className={table}>
        <caption className={showLabel === true ? CAPTION : "sr-only"}>
          {label}
        </caption>
        {children}
      </table>
    </div>
  );
}

// `align` shadows the presentational HTML attribute of the same name, which
// has been deprecated since HTML 4 and which nothing here should be setting.
export interface ThProps extends Omit<
  ThHTMLAttributes<HTMLTableCellElement>,
  "align"
> {
  align?: CellAlign | undefined;
  /** Right-aligned with tabular figures, for a column of amounts. */
  numeric?: boolean | undefined;
  /** The header exists for a screen reader but is not drawn — an actions
   *  column, a column of row checkboxes. The text still has to be there:
   *  a nameless column is announced as nothing at all. */
  hideLabel?: boolean | undefined;
}

/** How a header wins its own alignment back.
 *
 *  `numeric` and `align` used to reach the figures but not the header above
 *  them: `.table th { text-align: left }` was one class and one element,
 *  `.numeric` was one class, so the base rule outranked it and a `<Th numeric>`
 *  was never right-aligned — in any of the ten copies, or in the restyle that
 *  reproduced their ranking (D1.53). A column of amounts hung its heading over
 *  the wrong end of itself, which is the one place alignment carries meaning.
 *
 *  Matching the attribute as well as the element is one class and one
 *  attribute, which outranks the base rule's one class and one element: the
 *  same move `TableEmpty` makes below, and the same one the stylesheet made
 *  with `.table td.empty`. It only ever applies when a caller asked for an
 *  alignment, so a plain `<Th>` still reads left. */
const TH_ALIGN = {
  start: "[&[data-align]]:text-left",
  center: "[&[data-align]]:text-center",
  end: "[&[data-align]]:text-right",
} as const;

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
  // The same choice `cell` makes for the figures, said again at a weight the
  // base header rule cannot outrank. Explicit `align` wins over `numeric`,
  // which is the order the stylesheet resolved to.
  const chosen =
    align !== undefined ? align : numeric === true ? "end" : undefined;
  const classes = cx(
    cell(align, numeric, className),
    chosen === undefined ? undefined : TH_ALIGN[chosen],
  );
  return (
    <th
      scope={scope}
      {...(classes === "" ? {} : { className: classes })}
      {...(chosen === undefined ? {} : { "data-align": chosen })}
      {...rest}
    >
      {hideLabel === true ? (
        <span className="sr-only">{children}</span>
      ) : (
        children
      )}
    </th>
  );
}

export interface TdProps extends Omit<
  TdHTMLAttributes<HTMLTableCellElement>,
  "align"
> {
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

/** The cell rules above reach this `<td>` through the table, and a descendant
 *  utility outranks a plain one on the cell itself — so `py-8` here would lose
 *  to `[&_td]:py-3` and the empty state would silently take an ordinary row's
 *  height. Matching the attribute is one class and one attribute, which
 *  outranks both: the same move the stylesheet made with `.table td.empty`. */
const EMPTY =
  "[&[data-empty]]:px-4 [&[data-empty]]:py-8 [&[data-empty]]:text-center " +
  "[&[data-empty]]:text-tertiary [&[data-empty]]:border-b-0";

/** The "nothing here" row, inside `<tbody>`. Inside the table rather than
 *  beside it: a table whose explanation lives in a sibling paragraph reads,
 *  to anyone navigating by table, as a table with no rows and no reason. */
export function TableEmpty({ cols, children }: TableEmptyProps) {
  return (
    <tr>
      <td className={EMPTY} colSpan={cols} data-empty="">
        {children}
      </td>
    </tr>
  );
}

const ALIGN = {
  start: "text-left",
  center: "text-center",
  end: "text-right",
} as const;

function cell(
  align: CellAlign | undefined,
  numeric: boolean | undefined,
  className: string | undefined,
): string | undefined {
  // Alignment is chosen once rather than layered. The stylesheet resolved a
  // `numeric` cell that also carried an explicit `align` by source order —
  // `.center` was written after `.numeric`, so it won. Utilities have no such
  // order, so the same answer is decided here instead.
  const classes = [
    align !== undefined ? ALIGN[align] : numeric === true ? "text-right" : "",
    numeric === true ? "tabular-nums whitespace-nowrap" : "",
    className ?? "",
  ]
    .filter(Boolean)
    .join(" ");
  return classes === "" ? undefined : classes;
}
