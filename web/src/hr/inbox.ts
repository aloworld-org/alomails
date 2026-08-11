// The merge (alo HR, ADR 0035, wave B6.07): three queues read at once, one
// stream oldest-wait-first, and the counts that go on a badge.
//
// This is the whole of what `docs/design/hr.md` § "The approvals inbox" calls
// "the web composes it". The three modules answer independently and none of
// them knows the others exist; what is added here is the ordering, the counts,
// and one rule about failure:
//
// **A queue that fails does not empty the inbox.** The three reads are settled
// rather than raced, so an expense surface that is down still leaves a manager
// able to decide the leave in front of them, and the kinds that failed are
// named — never silently shown as zero, which would read as "nothing is
// waiting" when the truth is "nobody knows".
//
// Nothing here re-checks a permission and nothing here decides an order the
// server owns. The only judgement it makes is which wait is oldest.
import { useCallback, useEffect, useMemo, useState } from "react";

import type { Approval, ApprovalKind } from "../platform/approvals";
import { announceApprovalsChanged, onApprovalsChanged } from "./approvalsBus";
import { useApprovalQueues } from "./queues";

/** How many are waiting in each kind the caller can work. A kind that is not
 *  theirs is absent rather than `0`: "no expenses waiting" and "expenses are
 *  not yours to decide" are different sentences. */
export type ApprovalCounts = Partial<Record<ApprovalKind, number>>;

/** Everything the inbox and its badge read. */
export interface Inbox {
  /** False until the doors have answered. */
  ready: boolean;
  /** True while a read is in flight, including a reload after a decision. */
  loading: boolean;
  /** Whether this caller has any queue at all. */
  works: boolean;
  /** Everything waiting, oldest wait first. */
  items: Approval[];
  counts: ApprovalCounts;
  /** The sum — what a badge shows. */
  total: number;
  /** The kinds whose read failed, so the screen can say so rather than show a
   *  short list as if it were the whole one. */
  failed: ApprovalKind[];
  /** Re-reads every queue. */
  reload: () => void;
  /** Takes a decision on one row through the queue that owns it, then re-reads
   *  and tells the rest of the app. Rejects with the server's own failure so
   *  the caller can show its sentence. */
  decide: (item: Approval, verdict: "approve" | "reject", note: string) => Promise<void>;
}

/** Oldest wait first; a record that does not say when it was handed in sorts
 *  last, because an unknown wait must not jump a known one. */
function byWait(a: Approval, b: Approval): number {
  if (a.waitingSince === null) return b.waitingSince === null ? 0 : 1;
  if (b.waitingSince === null) return -1;
  return a.waitingSince.localeCompare(b.waitingSince);
}

/**
 * The unified approvals inbox: leave, expense claims and timesheet weeks in one
 * stream, for whichever of the three this caller may decide.
 */
export function useApprovalInbox(): Inbox {
  const { ready, queues } = useApprovalQueues();
  const [items, setItems] = useState<Approval[]>([]);
  const [counts, setCounts] = useState<ApprovalCounts>({});
  const [failed, setFailed] = useState<ApprovalKind[]>([]);
  const [loading, setLoading] = useState(false);
  const [revision, setRevision] = useState(0);

  const reload = useCallback(() => setRevision((r) => r + 1), []);

  useEffect(() => {
    if (!ready) return;
    let live = true;
    setLoading(true);
    void (async () => {
      // Settled, not raced: one queue's failure must not take the others' rows
      // off the screen.
      const answers = await Promise.allSettled(queues.map((queue) => queue.list()));
      if (!live) return;
      const rows: Approval[] = [];
      const tally: ApprovalCounts = {};
      const broken: ApprovalKind[] = [];
      answers.forEach((answer, index) => {
        const kind = queues[index]?.kind;
        if (kind === undefined) return;
        if (answer.status === "fulfilled") {
          rows.push(...answer.value);
          tally[kind] = answer.value.length;
        } else {
          broken.push(kind);
        }
      });
      setItems(rows.sort(byWait));
      setCounts(tally);
      setFailed(broken);
      setLoading(false);
    })();
    return () => {
      live = false;
    };
  }, [ready, queues, revision]);

  // A decision taken anywhere — this screen, the module's own approver screen,
  // another tab of the same session — is a count this one has to re-read.
  useEffect(() => onApprovalsChanged(reload), [reload]);

  const decide = useCallback(
    async (item: Approval, verdict: "approve" | "reject", note: string) => {
      const queue = queues.find((candidate) => candidate.kind === item.kind);
      if (queue === undefined) return;
      if (verdict === "approve") await queue.approve(item.id, note === "" ? undefined : note);
      else await queue.reject(item.id, note);
      // The badge elsewhere is re-read from the server, never decremented here.
      announceApprovalsChanged();
    },
    [queues],
  );

  return useMemo(
    () => ({
      ready,
      loading,
      works: queues.length > 0,
      items,
      counts,
      total: items.length,
      failed,
      reload,
      decide,
    }),
    [ready, loading, queues, items, counts, failed, reload, decide],
  );
}
