// The panels the traffic desk is built from. Presentational only: each one
// takes rows that are already named and ordered, and shows the top few with
// the rest one click away — a panel that dumps ten rows of every dimension at
// once stops being readable at exactly the moment the site gets traffic.
import { useState } from "react";
import type { ReactNode } from "react";

import { strings } from "../i18n";

/** One named row of a dimension: what it is, and how many views it got. */
export interface AnalyticsRow {
  label: string;
  visits: number;
}

/** How many rows a panel shows before "show all". Five fits beside three
 *  other panels without scrolling; the server never sends more than ten. */
const VISIBLE_ROWS = 5;

/** One dimension: a heading, the note that keeps it honest, and either the
 *  rows or the empty state that says how rows get here.
 *
 *  `ordered` panels (the reading-time histogram) keep the order they were
 *  given — a duration histogram sorted by count is not a histogram — and size
 *  their bars against the largest row rather than the first. */
export function DimensionPanel({
  title,
  note,
  empty,
  rows,
  ordered = false,
  numbers,
}: {
  title: string;
  note: string;
  empty: string;
  rows: AnalyticsRow[];
  ordered?: boolean;
  numbers: Intl.NumberFormat;
}) {
  const [expanded, setExpanded] = useState(false);
  // A histogram is shown whole or it is a different claim about the same
  // data, so `ordered` panels never collapse.
  const shown = expanded || ordered ? rows : rows.slice(0, VISIBLE_ROWS);
  const largest = rows.reduce((max, row) => Math.max(max, row.visits), 0);

  return (
    <section className="flex min-w-0 flex-col rounded-2xl border border-subtle bg-surface-raised p-5 shadow-sm">
      <div className="flex items-start justify-between gap-4">
        <h3 className="text-base font-semibold text-primary">{title}</h3>
      </div>
      <p className="mt-1.5 text-sm leading-5 text-secondary">{note}</p>
      {rows.length === 0 ? (
        <p className="mt-5 rounded-xl bg-surface px-4 py-5 text-sm leading-5 text-secondary">
          {empty}
        </p>
      ) : (
        <>
          <ol className="mt-5 space-y-3">
            {shown.map((row) => (
              <li
                key={row.label}
                className="grid grid-cols-[minmax(0,1fr)_5rem_auto] items-center gap-3"
              >
                <span
                  className="truncate text-sm font-medium text-primary"
                  title={row.label}
                >
                  {row.label}
                </span>
                <span
                  className="h-1.5 overflow-hidden rounded-full bg-surface"
                  aria-hidden="true"
                >
                  <span
                    className="block h-full rounded-full bg-accent"
                    style={{
                      width: `${largest === 0 ? 0 : (row.visits / largest) * 100}%`,
                    }}
                  />
                </span>
                <strong className="min-w-8 text-right text-sm tabular-nums text-primary">
                  {numbers.format(row.visits)}
                </strong>
              </li>
            ))}
          </ol>
          {rows.length > VISIBLE_ROWS && !ordered && (
            <button
              type="button"
              className="mt-5 self-start rounded-lg bg-surface px-3 py-2 text-sm font-medium text-primary transition-colors hover:bg-accent-soft hover:text-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent"
              aria-expanded={expanded}
              onClick={() => setExpanded((open) => !open)}
            >
              {expanded
                ? strings.sitesAnalyticsShowTop(VISIBLE_ROWS)
                : strings.sitesAnalyticsShowAll(rows.length)}
            </button>
          )}
        </>
      )}
    </section>
  );
}

/** A titled group of panels. The screen is four of these rather than one wall
 *  of numbers: an owner reads "how people found you", not "dimension 3". */
export function AnalyticsGroup({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <section className="space-y-3">
      <h2 className="text-lg font-semibold tracking-tight text-primary">
        {title}
      </h2>
      <div className="grid gap-4 lg:grid-cols-3">{children}</div>
    </section>
  );
}
