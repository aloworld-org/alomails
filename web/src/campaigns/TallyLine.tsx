// The count, and every person it leaves out (ADR 0044, C1.5).
//
// The item's rule, on screen: *the count and its exclusions are both readable;
// a number without them is not auditable.* So the exclusions are not a tooltip
// and not a link to a second page — they sit beside the number, each named,
// because "412 of 500" with no explanation is how somebody finds out by
// sending.
//
// The number is the server's. Nothing here adds anything up.
import { Badge, Spinner } from "../ds";
import { strings } from "../i18n";
import { exclusionLabel } from "./format";
import type { SegmentTally } from "./types";
import styles from "./CampaignsModule.module.css";

export function TallyLine({ tally, loading }: { tally: SegmentTally | null; loading: boolean }) {
  if (tally === null) {
    return (
      <p className={styles.tally} aria-busy={loading}>
        <Spinner />
      </p>
    );
  }

  if (tally.matched === 0) {
    return (
      <p className={styles.tally} aria-live="polite">
        {strings.campaignsTallyNobody}
      </p>
    );
  }

  return (
    <div className={styles.tally} aria-live="polite" aria-busy={loading}>
      <p className={styles.tallyCount}>
        {strings.campaignsTallyMailable(tally.mailable, tally.matched)}
      </p>
      {tally.excluded.length > 0 && (
        <ul className={styles.exclusions}>
          {tally.excluded.map((exclusion) => (
            <li key={exclusion.reason}>
              {/* Neutral, not a warning: somebody who unsubscribed is not an
                  error to be cleared, and a red badge would invite a colleague
                  to try to clear them. */}
              <Badge tone="neutral">
                {strings.campaignsExcludedCount(exclusion.people, exclusionLabel(exclusion.reason))}
              </Badge>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
