// What has already moved against an order: the arrivals booked against one we
// placed, the consignments sent against one a customer placed (B5.09b).
//
// Read-only, and one component for both, because a consignment is the same
// record in either direction — when, where, how much of which line. It is the
// document's own audit trail, and it is on the document rather than behind a
// tab: "we have had three of the ten" is the first thing a person opening a
// part-received order needs to see, and a screen that made them go looking for
// it is a screen that gets telephoned about.
//
// Nothing here is computed. Every quantity and every date arrives formatted by
// the screen that read it, so this file cannot disagree with the stock ledger
// that recorded the same movement.
import type { ReactNode } from "react";

import styles from "./InventoryModule.module.css";

/** One consignment, as this list shows it. */
export interface FulfilmentEntry {
  id: string;
  /** What it is called on paper — a delivery note number, or "Arrival 2". */
  title: string;
  /** The day it happened. */
  when: string;
  /** Where it came from, or went to: the tenant's own place. */
  place: string;
  note: string;
  lines: { key: string; description: string; qty: string }[];
  /** What else this consignment produced — the draft bill an arrival raised. */
  aside?: ReactNode;
}

export function FulfilmentList({
  title,
  empty,
  entries,
}: {
  title: string;
  empty: string;
  entries: FulfilmentEntry[];
}) {
  return (
    <section className={styles.lines}>
      <h2 className={styles.sectionTitle}>{title}</h2>
      {entries.length === 0 ? (
        <p className={styles.noMatches}>{empty}</p>
      ) : (
        <ul className={styles.fulfilList}>
          {entries.map((entry) => (
            <li key={entry.id} className={styles.fulfilEntry}>
              <div className={styles.fulfilHead}>
                <span className={styles.fulfilTitle}>{entry.title}</span>
                <span className={styles.subtleInline}>
                  {entry.when} · {entry.place}
                </span>
                <span className={styles.toolbarSpacer} />
                {entry.aside}
              </div>
              <ul className={styles.fulfilLines}>
                {entry.lines.map((line) => (
                  <li key={line.key}>
                    <span>{line.description}</span>
                    <span className={styles.numeric}>{line.qty}</span>
                  </li>
                ))}
              </ul>
              {entry.note !== "" && <p className={styles.fulfilNote}>{entry.note}</p>}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
