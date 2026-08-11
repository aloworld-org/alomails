// The count in the rail (alo HR, ADR 0035, wave B6.07) — the one HR component
// that lives outside its module, for the reason the running timer does: an
// inbox you can only see by opening it is an inbox that keeps people waiting.
//
// It is registered through the product surface (`web/src/product`), not
// imported by the shell: HR is a workspace module and the standalone mail
// product must not grow a dependency on it. The rail renders whatever widgets
// its surface declares and knows nothing about approvals.
//
// Three rules, and the first is what keeps it honest:
//
//   - **It draws nothing when nothing is waiting** — and nothing at all for the
//     majority of people, who have no queue to work. A permanent "0" in the
//     rail is a control that exists only to say no.
//   - **It polls nothing.** One read when it mounts, and one more whenever a
//     decision is announced (`approvalsBus`). A badge that ticked would be
//     three requests a minute for a number that changes twice a day.
//   - **The number is the queues' own.** It is re-read from the server after
//     every decision rather than decremented here, so it can never disagree
//     with the list it links to.
import { Link } from "react-router-dom";
import { Inbox } from "lucide-react";

import { strings } from "../i18n";
import { useApprovalInbox } from "./inbox";
import styles from "./ApprovalsWidget.module.css";

/** Where the badge goes: the one inbox, not the module that owns whichever
 *  record happens to be oldest. */
const INBOX = "/hr/approvals";

export function ApprovalsWidget() {
  const inbox = useApprovalInbox();

  // Nothing waiting is the ordinary state of a workspace, and no queue at all
  // is the ordinary state of a person.
  if (!inbox.ready || !inbox.works || inbox.total === 0) return null;

  return (
    <Link to={INBOX} className={styles.widget} title={strings.hrApprovalsWidgetTitle}>
      <Inbox size={13} aria-hidden="true" />
      <span className={styles.count}>{inbox.total}</span>
      <span className={styles.label}>{strings.hrApprovalsWidgetLabel}</span>
    </Link>
  );
}
