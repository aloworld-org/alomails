// The Finance module (alo Finance, ADR 0035, wave B4) — the workspace surface
// over the `/finance` API.
//
// It is mounted at `/finance/*` by the product surface, so every path below is
// relative and a deep link survives a page reload.
//
// **All four of the design note's tabs are drawn** — Expenses (B4.13a), Bank
// (B4.13b, as two: importing a month and working its lines), Accounts and
// Reports (B4.13c). What is deliberately still absent from Accounts is the
// journal behind the chart and the manual-entry dialog: `/finance/entries` has
// no HTTP door yet, and a screen for a route that does not exist is the promise
// this module does not make.
//
// **The bank is two tabs, not one.** Importing a month and working through its
// lines are different jobs done at different times — one is a file and a
// mapping, the other is an afternoon of small decisions — and a single screen
// with a mode would make the import banner the thing a bookkeeper scrolls past
// four hundred times.
//
// **Three tabs are hidden, not disabled, for anybody who is not a bookkeeper.**
// Deciding somebody's claim, staging a statement and settling a line are all
// the admin-or-accountant door (`docs/design/finance.md` § The accountant
// role), so a tab that exists only to refuse would be advertising a door this
// person does not have. The routes stay mounted: a bookmark works for the
// people who have it, and everybody else gets the server's own `403` on the
// read rather than a page pretending the queue is empty. The client is never
// the access decision — every one of those routes gates itself.
import { useCallback, useEffect, useMemo, useState } from "react";
import { NavLink, Navigate, Route, Routes } from "react-router-dom";

import { strings } from "../i18n";
import { useJmapClient } from "../jmap";
import { useProjects } from "../projects";
import { AccountsView } from "./AccountsView";
import { ApprovalsView } from "./ApprovalsView";
import { BankView } from "./BankView";
import { ExpensesView } from "./ExpensesView";
import { ReconcileView } from "./ReconcileView";
import { ReportsView } from "./ReportsView";
import type { ProjectChoice } from "./ExpenseDialog";
import styles from "./FinanceModule.module.css";

/**
 * Where the product surface mounts this module (`product/workplace.tsx`).
 *
 * **Every link and redirect below is absolute, and this is why.** The module is
 * mounted on a splat route (`/finance/*`), and react-router resolves a relative
 * `to` inside one against the *current location* rather than against the route
 * — so `to="expenses"` read from `/finance/bank` navigates to
 * `/finance/bank/expenses`, which matches the catch-all, which redirects
 * relatively again, and the address grows a segment per render. Stating the
 * root once makes every tab land where it says it lands, from any depth,
 * including the reports' own second row.
 */
const FINANCE_ROOT = "/finance";

/** The tabs, in the order the module is worked through: a claim, the decisions
 *  on it, the statement it turns up on, the matching, the chart it all books
 *  to, and the four reports. `bookkeeper` marks the ones only an admin or an
 *  accountant sees. */
const TABS: { path: string; label: () => string; bookkeeper: boolean }[] = [
  { path: "expenses", label: () => strings.financeTabExpenses, bookkeeper: false },
  { path: "approvals", label: () => strings.financeTabApprovals, bookkeeper: true },
  { path: "bank", label: () => strings.financeTabBank, bookkeeper: true },
  { path: "reconcile", label: () => strings.financeTabReconcile, bookkeeper: true },
  { path: "accounts", label: () => strings.financeTabAccounts, bookkeeper: true },
  { path: "reports", label: () => strings.financeTabReports, bookkeeper: true },
];

export function FinanceModule() {
  const client = useJmapClient();
  const { projects } = useProjects();
  const [approver, setApprover] = useState(false);
  // A decision on the approvals tab changes what the claimant's own list says
  // about their claim, and an import changes what the reconciliation screen has
  // to work through, so the screens share one counter rather than each
  // discovering the others' writes on a reload.
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
        {/* Scrolls horizontally on a phone by design; the responsive e2e
            sweep exempts marked strips from its width invariant. */}
        <nav className={styles.tabs} data-allow-overflow="">
          {TABS.filter((tab) => approver || !tab.bookkeeper).map((tab) => (
            <NavLink
              key={tab.path}
              to={`${FINANCE_ROOT}/${tab.path}`}
              className={({ isActive }) =>
                isActive ? `${styles.tab} ${styles.tabActive}` : styles.tab
              }
            >
              {tab.label()}
            </NavLink>
          ))}
        </nav>
      </header>

      <Routes>
        <Route index element={<Navigate to={`${FINANCE_ROOT}/expenses`} replace />} />
        <Route
          path="expenses"
          element={<ExpensesView projects={choices} revision={revision} />}
        />
        <Route path="approvals" element={<ApprovalsView onDecided={bump} />} />
        {/* An import grows the pile the reconciliation screen works through, so
            the two share the module's counter rather than each discovering the
            other's writes on a reload. */}
        <Route path="bank" element={<BankView onImported={bump} />} />
        <Route path="reconcile" element={<ReconcileView revision={revision} />} />
        {/* The chart and the four reports (B4.13c). Reports is a wildcard mount:
            it owns a second row of tabs of its own; the chart's editor is a
            dialog rather than a route. */}
        <Route path="accounts" element={<AccountsView />} />
        <Route path="reports/*" element={<ReportsView />} />
        {/* An unknown Finance path is a stale link, not an error page. */}
        <Route path="*" element={<Navigate to={`${FINANCE_ROOT}/expenses`} replace />} />
      </Routes>
    </div>
  );
}
