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
import {
  BadgeEuro,
  ChartNoAxesCombined,
  CircleGauge,
  Landmark,
  BriefcaseBusiness,
  ListChecks,
  ReceiptText,
  Scale,
  ShieldCheck,
  WalletCards,
  TrendingUp,
  LockKeyhole,
} from "lucide-react";

import { ModuleNavigation, moduleNavigationItemClassName } from "../ds";
import { strings } from "../i18n";
import { useJmapClient } from "../jmap";
import { useProjects } from "../projects";
import { AccountsView } from "./AccountsView";
import { ApprovalsView } from "./ApprovalsView";
import { BankView } from "./BankView";
import { CashFlowView } from "./CashFlowView";
import { ExpensesView } from "./ExpensesView";
import { FinanceOverviewView } from "./FinanceOverviewView";
import { ProjectProfitabilityView } from "./ProjectProfitabilityView";
import { SpendControlsView } from "./SpendControlsView";
import { CloseView } from "./CloseView";
import { ReconcileView } from "./ReconcileView";
import { ReportsView } from "./ReportsView";
import type { ProjectChoice } from "./ExpenseDialog";

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
const TABS = [
  {
    path: "overview",
    label: () => strings.financeTabOverview,
    Icon: CircleGauge,
    bookkeeper: true,
  },
  {
    path: "expenses",
    label: () => strings.financeTabExpenses,
    Icon: ReceiptText,
    bookkeeper: false,
  },
  {
    path: "approvals",
    label: () => strings.financeTabApprovals,
    Icon: ListChecks,
    bookkeeper: true,
  },
  {
    path: "bank",
    label: () => strings.financeTabBank,
    Icon: Landmark,
    bookkeeper: true,
  },
  {
    path: "reconcile",
    label: () => strings.financeTabReconcile,
    Icon: Scale,
    bookkeeper: true,
  },
  {
    path: "cash-flow",
    label: () => strings.financeTabCashFlow,
    Icon: TrendingUp,
    bookkeeper: true,
  },
  {
    path: "profitability",
    label: () => strings.financeTabProfitability,
    Icon: BriefcaseBusiness,
    bookkeeper: true,
  },
  {
    path: "controls",
    label: () => strings.financeTabControls,
    Icon: ShieldCheck,
    bookkeeper: true,
  },
  {
    path: "close",
    label: () => strings.financeTabClose,
    Icon: LockKeyhole,
    bookkeeper: true,
  },
  {
    path: "accounts",
    label: () => strings.financeTabAccounts,
    Icon: BadgeEuro,
    bookkeeper: true,
  },
  {
    path: "reports",
    label: () => strings.financeTabReports,
    Icon: ChartNoAxesCombined,
    bookkeeper: true,
  },
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
    <div className="flex h-full min-h-0 w-full flex-col bg-app">
      <header className="shrink-0 border-b border-subtle bg-surface px-8 pb-3 pt-5 max-sm:px-4 max-sm:pt-4">
        <div className="flex items-center gap-3">
          <span
            className="flex size-11 shrink-0 items-center justify-center rounded-xl bg-[var(--accent-soft)] text-accent ring-1 ring-inset ring-accent/10"
            aria-hidden="true"
          >
            <WalletCards className="size-5" />
          </span>
          <div className="min-w-0">
            <h1 className="m-0 text-2xl font-bold tracking-tight text-primary">
              {strings.moduleFinance}
            </h1>
            <p className="m-0 mt-1 text-sm text-secondary">
              {strings.financeWorkspacePurpose}
            </p>
          </div>
        </div>
        {/* Scrolls horizontally on a phone by design; the responsive e2e
            sweep exempts marked strips from its width invariant. */}
        <ModuleNavigation className="mt-4 gap-1" label={strings.moduleFinance}>
          {TABS.filter((tab) => approver || !tab.bookkeeper).map((tab) => (
            <NavLink
              key={tab.path}
              to={`${FINANCE_ROOT}/${tab.path}`}
              className={({ isActive }) =>
                moduleNavigationItemClassName(isActive)
              }
            >
              <tab.Icon className="size-4" aria-hidden="true" />
              {tab.label()}
            </NavLink>
          ))}
        </ModuleNavigation>
      </header>

      <Routes>
        <Route
          index
          element={
            <Navigate
              to={`${FINANCE_ROOT}/${approver ? "overview" : "expenses"}`}
              replace
            />
          }
        />
        <Route path="overview" element={<FinanceOverviewView />} />
        <Route
          path="expenses"
          element={<ExpensesView projects={choices} revision={revision} />}
        />
        <Route path="approvals" element={<ApprovalsView onDecided={bump} />} />
        {/* An import grows the pile the reconciliation screen works through, so
            the two share the module's counter rather than each discovering the
            other's writes on a reload. */}
        <Route path="bank" element={<BankView onImported={bump} />} />
        <Route
          path="reconcile"
          element={<ReconcileView revision={revision} />}
        />
        <Route path="cash-flow" element={<CashFlowView />} />
        <Route path="profitability" element={<ProjectProfitabilityView />} />
        <Route path="controls" element={<SpendControlsView />} />
        <Route path="close" element={<CloseView />} />
        {/* The chart and the four reports (B4.13c). Reports is a wildcard mount:
            it owns a second row of tabs of its own; the chart's editor is a
            dialog rather than a route. */}
        <Route path="accounts" element={<AccountsView />} />
        <Route path="reports/*" element={<ReportsView />} />
        {/* An unknown Finance path is a stale link, not an error page. */}
        <Route
          path="*"
          element={
            <Navigate
              to={`${FINANCE_ROOT}/${approver ? "overview" : "expenses"}`}
              replace
            />
          }
        />
      </Routes>
    </div>
  );
}
