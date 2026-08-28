// The HR module (alo HR, ADR 0035, wave B6) — the workspace surface over the
// `/hr` API. It is mounted at `/hr/*` by the product surface, so every path
// below is relative and a deep link survives a page reload.
//
// **The rail entry is visible to every member, and that is deliberate**
// (`docs/design/hr.md` § Web surface): the most-used screen in HR is an
// employee's own — how much leave have I got left — and hiding the module
// behind a role would make that a question you ask a person. What varies by
// door is the tabs.
//
// Today there are five tabs, and the first three are everybody's:
//
//   - **My leave** — the balance with its working, the request form, and this
//     person's own requests (B6.08b); for anybody who decides leave, the same
//     list one relationship wider. It is what a member lands on, because it is
//     the reason the rail entry is every member's in the first place.
//   - **Who's away** — the absence calendar, the module's one screen about
//     other people and every member's read (B6.08b).
//   - **Directory** — the people list and the org chart (B6.08a), read with no
//     door at all.
//   - **Hiring** — the admin-or-HR door, and what HR still lands on: an
//     approvals inbox is usually empty, and a module that opened on an empty
//     screen would read as a module with nothing in it.
//   - **Approvals** — the one inbox (B6.07), drawn for anybody who has
//     something to decide: HR, a tenant admin, an accountant, or simply
//     somebody with a direct report. It is deliberately not an HR-role tab,
//     because most of what it holds is not HR's (`queues.ts`). A manager who is
//     not HR lands on the inbox.
//
// The design note's separate **Team** tab is deliberately not built: both
// halves of it — the reports' requests and their booked absence — are the same
// two screens above asked a different way (a scope switch, and a calendar every
// member already has), and a third place to decide leave is a third place to
// forget one.
//
// Tabs are hidden, not disabled, for anybody without the door — the pattern
// Finance set for its bookkeeper tabs. The routes stay mounted, so a bookmark
// works for the people who hold the door, and everybody else gets the server's
// own `403` on the read rather than a page pretending a queue is empty.
// **The client is never the access decision**: every `/hr` route asks
// `require_hr` again for itself, and the three approval queues each ask their
// own.
import { useEffect, useState } from "react";
import { NavLink, Navigate, Route, Routes } from "react-router-dom";

import { Spinner } from "../ds";
import { strings } from "../i18n";
import { useJmapClient } from "../jmap";
import { ApprovalsView } from "./ApprovalsView";
import { AwayView } from "./AwayView";
import { DirectoryView } from "./DirectoryView";
import { HiringView } from "./HiringView";
import { LeaveView } from "./LeaveView";
import { LetterTemplatesView } from "./LetterTemplatesView";
import { useApprovalQueues } from "./queues";
import styles from "./hr.module.css";

/**
 * Where the product surface mounts this module.
 *
 * Every link below is absolute, for the reason Finance records: the module is
 * mounted on a splat route, and react-router resolves a relative `to` against
 * the *current location* rather than the route — so a relative tab link read
 * from a nested path grows a segment per press.
 */
const HR_ROOT = "/hr";

export function HrModule() {
  const client = useJmapClient();
  // `null` = the door has not answered yet. It matters: starting at "not HR"
  // would flash the member's page at a recruiter on every load, and starting at
  // "HR" would flash a board at somebody who has no business seeing one.
  const [hr, setHr] = useState<boolean | null>(null);
  // The same reasoning for the inbox: `ready` is false until every door has
  // answered, and a tab is drawn from the answer rather than from a guess.
  const approvals = useApprovalQueues();
  const canApprove = approvals.ready && approvals.queues.length > 0;
  const settled = hr !== null && approvals.ready;

  useEffect(() => {
    let live = true;
    void client
      .canWorkHr()
      .then((ok) => {
        if (live) setHr(ok);
      })
      .catch(() => {
        // Not HR, or the check is unavailable → the tab stays hidden, which is
        // the same thing a refusal would mean.
        if (live) setHr(false);
      });
    return () => {
      live = false;
    };
  }, [client]);

  return (
    <div className={styles.hr}>
      <header className={styles.header}>
        <h1 className={styles.title}>{strings.moduleHr}</h1>
        {!settled && <Spinner size={16} />}
        {settled && (
          // Scrolls horizontally on a phone by design; the responsive e2e
          // sweep exempts marked strips from its width invariant.
          <nav className={styles.tabs} data-allow-overflow="">
            {/* Everybody's, and first: a person's own leave is the most-read
                thing in HR and the reason this module is not behind a role. */}
            <NavLink
              to={`${HR_ROOT}/leave`}
              className={({ isActive }) =>
                isActive ? `${styles.tab} ${styles.tabActive}` : styles.tab
              }
            >
              {strings.hrTabLeave}
            </NavLink>
            <NavLink
              to={`${HR_ROOT}/away`}
              className={({ isActive }) =>
                isActive ? `${styles.tab} ${styles.tabActive}` : styles.tab
              }
            >
              {strings.hrTabAway}
            </NavLink>
            <NavLink
              to={`${HR_ROOT}/directory`}
              className={({ isActive }) =>
                isActive ? `${styles.tab} ${styles.tabActive}` : styles.tab
              }
            >
              {strings.hrTabDirectory}
            </NavLink>
            {hr === true && (
              <NavLink
                to={`${HR_ROOT}/templates`}
                className={({ isActive }) =>
                  isActive ? `${styles.tab} ${styles.tabActive}` : styles.tab
                }
              >
                {strings.hrTabTemplates}
              </NavLink>
            )}
            {hr === true && (
              <NavLink
                to={`${HR_ROOT}/hiring`}
                className={({ isActive }) =>
                  isActive ? `${styles.tab} ${styles.tabActive}` : styles.tab
                }
              >
                {strings.hrTabHiring}
              </NavLink>
            )}
            {canApprove && (
              <NavLink
                to={`${HR_ROOT}/approvals`}
                className={({ isActive }) =>
                  isActive ? `${styles.tab} ${styles.tabActive}` : styles.tab
                }
              >
                {strings.hrTabApprovals}
              </NavLink>
            )}
          </nav>
        )}
      </header>

      <Routes>
        <Route
          index
          element={
            // The screen this person came for, which is not the first tab they
            // have. HR keeps landing on the board it has always landed on — an
            // approvals inbox is usually empty, and a module that opened on an
            // empty screen would read as a module with nothing in it. Somebody
            // who has decisions waiting lands on them. Everybody else lands on
            // their own leave: how much have I got left, and can I have next
            // Thursday off, is what most people open HR to ask.
            !settled ? null : hr ? (
              <Navigate to={`${HR_ROOT}/hiring`} replace />
            ) : canApprove ? (
              <Navigate to={`${HR_ROOT}/approvals`} replace />
            ) : (
              <Navigate to={`${HR_ROOT}/leave`} replace />
            )
          }
        />
        <Route path="approvals" element={<ApprovalsView />} />
        <Route path="away" element={<AwayView />} />
        <Route path="directory" element={<DirectoryView />} />
        <Route path="leave" element={<LeaveView />} />
        <Route path="hiring" element={<HiringView />} />
        {hr === true && (
          <Route path="templates" element={<LetterTemplatesView />} />
        )}
        {/* An unknown HR path is a stale link, not an error page. */}
        <Route path="*" element={<Navigate to={HR_ROOT} replace />} />
      </Routes>
    </div>
  );
}
