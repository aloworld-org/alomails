// What each queue is called in the inbox (alo HR, ADR 0035, wave B6.07).
//
// Its own file rather than a case in `format.ts`, because this is the one place
// the three modules are named as a set — and the words are chosen from the
// approver's side of each: what is waiting is "time off", "a claim", "a week",
// not "an HR leave request record".
import { strings } from "../i18n";
import type { ApprovalKind } from "../platform/approvals";

/** The word for a queue. Exhaustive over the three kinds, so a fourth queue
 *  cannot be added without deciding what it is called. */
export function kindLabel(kind: ApprovalKind): string {
  switch (kind) {
    case "leave":
      return strings.hrQueueLeave;
    case "expense":
      return strings.hrQueueExpense;
    case "timesheet":
      return strings.hrQueueTimesheet;
  }
}
