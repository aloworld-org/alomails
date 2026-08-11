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
// Today there is exactly one tab, **Hiring**, and it is the admin-or-HR door.
// The member-facing tabs (My leave, Team, Directory) are the wave's later web
// items and are not drawn as empty promises: a member who opens HR now is told
// what this module is and what is not here yet, in the module's own words,
// rather than shown a tab that answers nothing.
//
// The tab is hidden, not disabled, for anybody who is not HR — the pattern
// Finance set for its bookkeeper tabs. The route stays mounted, so a bookmark
// works for the people who hold the door, and everybody else gets the server's
// own `403` on the read rather than a page pretending a hiring board is empty.
// **The client is never the access decision**: every `/hr` route asks
// `require_hr` again for itself.
import { useEffect, useState } from "react";
import { NavLink, Navigate, Route, Routes } from "react-router-dom";
import { Users } from "lucide-react";

import { Spinner } from "../ds";
import { strings } from "../i18n";
import { useJmapClient } from "../jmap";
import { ComingSoon } from "../shell";
import { HiringView } from "./HiringView";
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
        {hr === null && <Spinner size={16} />}
        {hr === true && (
          <nav className={styles.tabs}>
            <NavLink
              to={`${HR_ROOT}/hiring`}
              className={({ isActive }) =>
                isActive ? `${styles.tab} ${styles.tabActive}` : styles.tab
              }
            >
              {strings.hrTabHiring}
            </NavLink>
          </nav>
        )}
      </header>

      <Routes>
        <Route
          index
          element={
            hr === null ? null : hr ? (
              <Navigate to={`${HR_ROOT}/hiring`} replace />
            ) : (
              <ComingSoon
                Icon={Users}
                title={strings.hrMemberSoonTitle}
                body={strings.hrMemberSoonBody}
              />
            )
          }
        />
        <Route path="hiring" element={<HiringView />} />
        {/* An unknown HR path is a stale link, not an error page. */}
        <Route path="*" element={<Navigate to={HR_ROOT} replace />} />
      </Routes>
    </div>
  );
}
