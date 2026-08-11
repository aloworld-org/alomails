// Approvals — the one inbox (alo HR, ADR 0035, wave B6.07): the leave, the
// expense claims and the timesheet weeks waiting for this person, in one list,
// oldest wait first.
//
// It exists because the three are one job. A manager on Monday morning does not
// think "I will do Finance now and Projects afterwards"; they think "what is
// waiting for me". Before this screen the answer lived in three modules, and the
// one that was easiest to forget was the one that made somebody wait longest.
//
// What the screen adds is ordering, counting and one honest failure mode; what
// it deliberately does not add is a rule:
//
//   - **No decision is taken here.** Every Approve and Send back travels the
//     owning module's own already-gated route (`platform/approvals.ts`), so the
//     three doors stay exactly where they were, beside the data they guard.
//   - **Nothing is filtered by person.** Each queue already answered only what
//     this caller may decide.
//   - **A queue that fails is named, never shown as empty.** A list that is
//     silently short reads as "nothing is waiting", which is the one wrong thing
//     an inbox can say.
//   - **Sending back asks for a sentence**, although all three servers accept an
//     empty one: the person whose week or claim comes back is going to read it,
//     and a refusal with no reason is somebody being made to guess.
//
// Approving is not confirmed — it is the ordinary act this screen exists for
// (`docs/design/ux-principles.md`, undo over confirm) — and the row leaves the
// list because the server said so on the re-read, never because this screen
// removed it.
import { useState } from "react";
import { Inbox as InboxIcon } from "lucide-react";
import { Link } from "react-router-dom";

import { Button, Spinner, useDialogs } from "../ds";
import { strings } from "../i18n";
import type { Approval } from "../platform/approvals";
import { hrMessage } from "./api";
import { momentLabel } from "./format";
import { useApprovalInbox } from "./inbox";
import { kindLabel } from "./queueLabels";
import { Chip, EmptyState, ErrorBanner } from "./parts";
import styles from "./hr.module.css";

export function ApprovalsView() {
  const inbox = useApprovalInbox();
  const dialogs = useDialogs();
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  /** A row's key, and the id of the row that is mid-decision: two kinds may
   *  serve the same id, so neither is unique on its own. */
  function rowKey(item: Approval): string {
    return `${item.kind}:${item.id}`;
  }

  async function decide(item: Approval, verdict: "approve" | "reject") {
    let note = "";
    if (verdict === "reject") {
      const written = await dialogs.prompt({
        title: strings.hrSendBackTitle,
        message: strings.hrSendBackBody(item.person),
        confirmLabel: strings.hrSendBack,
        placeholder: strings.hrSendBackPlaceholder,
      });
      // `null` is a cancelled prompt — not an empty note.
      if (written === null) return;
      note = written;
    }
    setBusy(rowKey(item));
    setError(null);
    try {
      await inbox.decide(item, verdict, note);
    } catch (err) {
      setError(hrMessage(err, strings.hrSaveFailed));
    } finally {
      setBusy(null);
    }
  }

  if (!inbox.ready || (inbox.loading && inbox.items.length === 0)) {
    return (
      <div className={styles.inbox}>
        <Spinner size={20} />
      </div>
    );
  }

  // Somebody with no queue at all is most of a company, and they are told what
  // this screen is rather than shown an empty table of somebody else's work.
  if (!inbox.works) {
    return (
      <div className={styles.inbox}>
        <EmptyState
          Icon={InboxIcon}
          title={strings.hrApprovalsNoneTitle}
          body={strings.hrApprovalsNoneBody}
        />
      </div>
    );
  }

  const kinds = Object.entries(inbox.counts) as [Approval["kind"], number][];
  return (
    <div className={styles.inbox}>
      {error !== null && <ErrorBanner message={error} />}
      {inbox.failed.length > 0 && (
        <ErrorBanner
          message={strings.hrApprovalsQueueFailed(
            inbox.failed.map(kindLabel).join(", "),
          )}
        />
      )}

      <div className={styles.counts}>
        <strong className={styles.countTotal}>{strings.hrWaitingCount(inbox.total)}</strong>
        {kinds.map(([kind, count]) => (
          <span key={kind} className={styles.count}>
            {strings.hrCountOf(kindLabel(kind), count)}
          </span>
        ))}
      </div>

      {inbox.items.length === 0 ? (
        <EmptyState
          Icon={InboxIcon}
          title={strings.hrApprovalsEmptyTitle}
          body={strings.hrApprovalsEmptyBody}
        />
      ) : (
        <div className={styles.tableWrap}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th scope="col">{strings.hrPerson}</th>
                <th scope="col">{strings.hrWhat}</th>
                <th scope="col">{strings.hrQueue}</th>
                <th scope="col" className={styles.numeric}>
                  {strings.hrFigure}
                </th>
                <th scope="col">{strings.hrWaitingSince}</th>
                <th scope="col" aria-label={strings.hrActions} />
              </tr>
            </thead>
            <tbody>
              {inbox.items.map((item) => (
                <tr key={rowKey(item)}>
                  <td>{item.person}</td>
                  <td>
                    <Link className={styles.rowLink} to={item.href}>
                      {item.what}
                    </Link>
                    {item.detail !== "" && <span className={styles.subtle}>{item.detail}</span>}
                  </td>
                  <td>
                    <Chip tone="info">{kindLabel(item.kind)}</Chip>
                  </td>
                  <td className={styles.numeric}>{item.figure}</td>
                  <td className={styles.muted}>
                    {item.waitingSince === null ? "" : momentLabel(item.waitingSince)}
                  </td>
                  <td>
                    <div className={styles.rowActions}>
                      <Button
                        variant="ghost"
                        size="sm"
                        disabled={busy !== null}
                        onClick={() => void decide(item, "reject")}
                      >
                        {strings.hrSendBack}
                      </Button>
                      <Button
                        size="sm"
                        disabled={busy !== null}
                        onClick={() => void decide(item, "approve")}
                      >
                        {strings.hrApprove}
                      </Button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
