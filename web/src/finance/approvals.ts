// Finance's expense claims, as a queue the unified approvals inbox can work
// (alo HR, ADR 0035, wave B6.07) — the sibling of `projects/approvals.ts`, and
// thin for the same two reasons.
//
// The module keeps its own approver screen, and it is the fuller one: it also
// holds the second list (what the company has approved and still owes), and
// paying somebody back happens there. **Reimbursement is deliberately not in
// this queue.** An inbox is a list of decisions; a payment is not a decision,
// and putting "mark paid back" beside "approve" would invite pressing one while
// meaning the other. `docs/design/hr.md` § "The approvals inbox" records the
// cut.
//
// The money is formatted **here**, by Finance's own formatter over the server's
// integer cents. Nothing downstream sees a number it could round.
import { useMemo } from "react";

import type { Approval, ApprovalQueue } from "../platform/approvals";
import { strings } from "../i18n";
import { useFinanceApi } from "./api";
import { amountLabel, dayLabel } from "./format";
import type { PendingExpense } from "./types";

/** Where an approver goes for the claim itself, and for what is owed. */
const EXPENSES_HREF = "/finance/approvals";

/** One waiting claim as a row of the shared inbox. */
function asApproval(claim: PendingExpense): Approval {
  const merchant = claim.merchant === "" ? strings.financeNoMerchant : claim.merchant;
  return {
    kind: "expense",
    id: claim.id,
    person: claim.userEmail,
    what: strings.financeClaimOf(merchant, dayLabel(claim.spentOn)),
    detail: claim.description === "" ? (claim.categoryName ?? "") : claim.description,
    figure: amountLabel(claim.grossCents, claim.currency),
    waitingSince: claim.submittedAt,
    href: EXPENSES_HREF,
  };
}

/**
 * The expense claims awaiting this caller, as an {@link ApprovalQueue}.
 *
 * Memoized on the client, so an inbox effect keyed on the queue does not loop.
 */
export function useExpenseApprovals(): ApprovalQueue {
  const api = useFinanceApi();
  return useMemo(
    () => ({
      kind: "expense" as const,
      list: () => api.pendingExpenses().then((claims) => claims.map(asApproval)),
      approve: async (id: string, note?: string) => {
        await api.approveExpense(id, note);
      },
      reject: async (id: string, note: string) => {
        await api.rejectExpense(id, note);
      },
    }),
    [api],
  );
}
