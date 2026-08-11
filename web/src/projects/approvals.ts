// Projects' timesheet weeks, as a queue the unified approvals inbox can work
// (alo HR, ADR 0035, wave B6.07).
//
// The module keeps its own approvals screen: a manager who is looking at
// engagements decides weeks where the hours are. This adapter is the same
// queue, put into the shared words of `platform/approvals` so one inbox can
// show it beside leave and expense claims — and it is deliberately thin.
//
// Two things it does NOT do, both on purpose:
//
//   - **It decides nothing about who may see this.** `/projects/approvals` is
//     the tenant admin's door and refuses everybody else itself; this file
//     would be the wrong place to hold a copy of that rule.
//   - **It formats the hours here, not in the inbox.** A week's minutes are
//     Projects' number and `durationLabel` is how this product has always
//     written them; an inbox that reformatted them would be a second answer to
//     "how long is 450 minutes".
import { useMemo } from "react";

import type { Approval, ApprovalQueue } from "../platform/approvals";
import { strings } from "../i18n";
import { useProjectsApi } from "./api";
import { dayLabel, durationLabel } from "./format";
import type { PendingWeek } from "./types";

/** Where a manager goes for the week itself: the module's own approvals
 *  screen, which shows every waiting week with its hours. */
const WEEKS_HREF = "/projects/approvals";

/** One handed-in week as a row of the shared inbox. */
function asApproval(week: PendingWeek): Approval {
  return {
    kind: "timesheet",
    id: week.id,
    person: week.userEmail,
    what: strings.projectsWeekOf(
      dayLabel(week.weekStart, { day: "numeric", month: "short" }),
      dayLabel(week.weekEnd),
    ),
    detail: strings.projectsBillableOf(durationLabel(week.billableMinutes)),
    figure: durationLabel(week.minutes),
    waitingSince: week.submittedAt,
    href: WEEKS_HREF,
  };
}

/**
 * The timesheet weeks awaiting this caller, as an {@link ApprovalQueue}.
 *
 * Memoized on the client, so an inbox effect keyed on the queue does not loop.
 */
export function useWeekApprovals(): ApprovalQueue {
  const api = useProjectsApi();
  return useMemo(
    () => ({
      kind: "timesheet" as const,
      list: () => api.approvals().then((weeks) => weeks.map(asApproval)),
      approve: async (id: string, note?: string) => {
        await api.approveWeek(id, note);
      },
      reject: async (id: string, note: string) => {
        await api.rejectWeek(id, note);
      },
    }),
    [api],
  );
}
