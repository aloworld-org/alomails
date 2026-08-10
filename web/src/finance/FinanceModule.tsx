// The Finance module (alo Finance, ADR 0035, wave B4) — the workspace surface
// over the `/finance` API.
//
// It is mounted at `/finance/*` by the product surface, so every path below is
// relative and a deep link survives a page reload.
//
// **This is the expenses slice (B4.13a).** The design note's four tabs are
// Expenses, Bank, Accounts and Reports; the three that read the ledger are
// B4.13b and B4.13c and are not drawn here, because a tab that opens an empty
// screen is a promise the module has not kept. They join this nav when they are
// built, which is why the nav is a list and not two hardcoded links.
//
// **Approvals is hidden, not disabled, for anybody who is not an approver.**
// Deciding somebody's claim is the admin-or-accountant door
// (`docs/design/finance.md` § The accountant role), so a tab that exists only to
// refuse would be advertising a door this person does not have. The route stays
// mounted: a bookmark works for the people who have it, and everybody else gets
// the server's own `403` on the read rather than a page pretending the queue is
// empty.
import { useCallback, useEffect, useMemo, useState } from "react";
import { NavLink, Navigate, Route, Routes } from "react-router-dom";

import { strings } from "../i18n";
import { useJmapClient } from "../jmap";
import { useProjects } from "../projects";
import { ApprovalsView } from "./ApprovalsView";
import { ExpensesView } from "./ExpensesView";
import type { ProjectChoice } from "./ExpenseDialog";
import styles from "./FinanceModule.module.css";

export function FinanceModule() {
  const client = useJmapClient();
  const { projects } = useProjects();
  const [approver, setApprover] = useState(false);
  // A decision on the approvals tab changes what the claimant's own list says
  // about their claim, so the two screens share one counter rather than each
  // discovering the other's writes on a reload.
  const [revision, setRevision] = useState(0);
  const bump = useCallback(() => setRevision((r) => r + 1), []);

  useEffect(() => {
    let live = true;
    void client
      .canWorkTheBooks()
      .then((ok) => {
        if (live) setApprover(ok);
      })
      .catch(() => {
        // Not an approver, or the check is unavailable → the tab stays hidden,
        // which is the same thing a refusal would mean.
      });
    return () => {
      live = false;
    };
  }, [client]);

  /** The engagements a claim can be attached to, as the picker needs them: an
   *  id and a name, and nothing about anybody's hours. */
  const choices = useMemo<ProjectChoice[]>(
    () => projects.map((project) => ({ id: project.id, name: project.name })),
    [projects],
  );

  return (
    <div className={styles.finance}>
      <header className={styles.header}>
        <h1 className={styles.title}>{strings.moduleFinance}</h1>
        <nav className={styles.tabs}>
          <NavLink
            to="expenses"
            className={({ isActive }) =>
              isActive ? `${styles.tab} ${styles.tabActive}` : styles.tab
            }
          >
            {strings.financeTabExpenses}
          </NavLink>
          {approver && (
            <NavLink
              to="approvals"
              className={({ isActive }) =>
                isActive ? `${styles.tab} ${styles.tabActive}` : styles.tab
              }
            >
              {strings.financeTabApprovals}
            </NavLink>
          )}
        </nav>
      </header>

      <Routes>
        <Route index element={<Navigate to="expenses" replace />} />
        <Route
          path="expenses"
          element={<ExpensesView projects={choices} revision={revision} />}
        />
        <Route path="approvals" element={<ApprovalsView onDecided={bump} />} />
        {/* An unknown Finance path is a stale link, not an error page. */}
        <Route path="*" element={<Navigate to="expenses" replace />} />
      </Routes>
    </div>
  );
}
