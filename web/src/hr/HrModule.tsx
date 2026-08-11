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
// Today there are three tabs, and the first of them is everybody's:
//
//   - **Directory** — the people list and the org chart (B6.08a), drawn for
//     every member and read with no door at all. It is what a member lands on
//     when nothing else is theirs, which is why the module no longer promises a
//     screen it does not have.
//   - **Hiring** — the admin-or-HR door, and what HR still lands on: an
//     approvals inbox is usually empty, and a module that opened on an empty
//     screen would read as a module with nothing in it.
//   - **Approvals** — the one inbox (B6.07), drawn for anybody who has
//     something to decide: HR, a tenant admin, an accountant, or simply
//     somebody with a direct report. It is deliberately not an HR-role tab,
//     because most of what it holds is not HR's (`queues.ts`). A manager who is
//     not HR has this tab and the directory, and lands on the inbox.
//
// The remaining member-facing tabs (My leave, Team) are the wave's later web
// items and are not drawn as empty promises.
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
import { DirectoryView } from "./DirectoryView";
import { HiringView } from "./HiringView";
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
          <nav className={styles.tabs}>
            {/* Everybody's, and first: the one HR screen that is not about a
                door but about the company. */}
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
            // the directory, because looking a colleague up is what most people
            // open HR to do.
            !settled ? null : hr ? (
              <Navigate to={`${HR_ROOT}/hiring`} replace />
            ) : canApprove ? (
              <Navigate to={`${HR_ROOT}/approvals`} replace />
            ) : (
              <Navigate to={`${HR_ROOT}/directory`} replace />
            )
          }
        />
        <Route path="approvals" element={<ApprovalsView />} />
        <Route path="directory" element={<DirectoryView />} />
        <Route path="hiring" element={<HiringView />} />
        {/* An unknown HR path is a stale link, not an error page. */}
        <Route path="*" element={<Navigate to={HR_ROOT} replace />} />
      </Routes>
    </div>
  );
}
