// The panels the traffic desk is built from. Presentational only: each one
// takes rows that are already named and ordered, and shows the top few with
// the rest one click away — a panel that dumps ten rows of every dimension at
// once stops being readable at exactly the moment the site gets traffic.
import { useState } from "react";
import type { CSSProperties, ReactNode } from "react";

import { strings } from "../i18n";
import styles from "./SitesModule.module.css";

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
    <section className={styles.analyticsPanel}>
      <div className={styles.analyticsPanelHead}>
        <h3>{title}</h3>
      </div>
      <p className={styles.analyticsNote}>{note}</p>
      {rows.length === 0 ? (
        <p className={styles.analyticsPanelEmpty}>{empty}</p>
      ) : (
        <>
          <ol className={styles.analyticsDimension}>
            {shown.map((row) => (
              <li key={row.label}>
                <span className={styles.analyticsDimensionLabel} title={row.label}>
                  {row.label}
                </span>
                <span
                  className={styles.analyticsShare}
                  aria-hidden="true"
                  style={
                    {
                      "--analytics-value":
                        largest === 0 ? 0 : row.visits / largest,
                    } as CSSProperties
                  }
                />
                <strong>{numbers.format(row.visits)}</strong>
              </li>
            ))}
          </ol>
          {rows.length > VISIBLE_ROWS && !ordered && (
            <button
              type="button"
              className={styles.analyticsMore}
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
    <section className={styles.analyticsGroup}>
      <h2 className={styles.analyticsGroupTitle}>{title}</h2>
      <div className={styles.analyticsRankings}>{children}</div>
    </section>
  );
}
