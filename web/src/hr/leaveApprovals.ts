// HR's own queue for the unified inbox: the leave somebody has asked for and
// nobody has decided (alo HR, ADR 0035, wave B6.07).
//
// The sibling of `projects/approvals.ts` and `finance/approvals.ts`, with one
// difference that is worth stating because it is not a detail: **the scope is
// the caller's relationship to the person, not a role.** A manager asks as
// `team` and gets their direct reports; HR asks as `all` and gets the tenant.
// Both are resolved server-side by `hr_leave_door.rs` from the org chart — this
// file chooses which question to ask and never which answer to trust.
//
// The days are the server's fold over the person's working pattern and the
// tenant's public holidays; the note is what the person wrote for whoever
// decides, shown to them and logged nowhere.
import { useMemo } from "react";

import type { Approval, ApprovalQueue } from "../platform/approvals";
import { strings } from "../i18n";
import { useHrApi } from "./api";
import { dayLabel } from "./format";
import type { HrLeaveRequest } from "./types";

/** Where the request itself lives once the member screens exist (B6.08b). The
 *  inbox is the screen today, so the link is the inbox. */
const LEAVE_HREF = "/hr/approvals";

/** One asked-for absence as a row of the shared inbox. */
function asApproval(request: HrLeaveRequest): Approval {
  return {
    kind: "leave",
    id: request.id,
    person: request.employeeName,
    what: strings.hrLeaveOf(request.policyName, dayLabel(request.fromDay), dayLabel(request.toDay)),
    detail: request.note,
    figure: strings.hrWorkingDays(request.workingDays),
    waitingSince: request.createdAt,
    href: LEAVE_HREF,
  };
}

/**
 * The leave awaiting this caller's decision, as an {@link ApprovalQueue}.
 *
 * @param scope `team` for a manager, `all` for HR — the question this queue
 *   asks. Handed in rather than decided here, because who the caller is was
 *   already resolved once (`useApprovalQueues`).
 */
export function useLeaveApprovals(scope: "team" | "all"): ApprovalQueue {
  const api = useHrApi();
  return useMemo(
    () => ({
      kind: "leave" as const,
      list: () =>
        api
          .leaveRequests(scope, ["requested"])
          .then((requests) => requests.map(asApproval)),
      approve: async (id: string, note?: string) => {
        await api.approveLeaveRequest(id, note);
      },
      reject: async (id: string, note: string) => {
        await api.rejectLeaveRequest(id, note);
      },
    }),
    [api, scope],
  );
}
