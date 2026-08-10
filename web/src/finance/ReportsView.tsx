// The Reports tab: the four folds of the journal a business is actually asked
// for — the result, the position, who owes what, and the tax.
//
// A second row of tabs rather than four entries in the module's own nav: they
// are one job ("look at the books") done four ways, and putting them beside
// Expenses and Bank would make the module's nav a list of eight things at the
// moment somebody is looking for two. Each report keeps its own route, so a
// link to the VAT return is a link to the VAT return.
import { NavLink, Navigate, Route, Routes } from "react-router-dom";

import { strings } from "../i18n";
import { AgedReportView } from "./AgedReportView";
import { BalanceSheetView } from "./BalanceSheetView";
import { PlReportView } from "./PlReportView";
import { VatReturnView } from "./VatReturnView";
import styles from "./FinanceModule.module.css";

/** Where this second row lives, stated once and absolutely — for the reason
 *  `FinanceModule`'s own root is (react-router resolves a relative `to` inside
 *  a splat route against the current location, which on a nested tab row grows
 *  a segment per click). */
const REPORTS_ROOT = "/finance/reports";

/** The four, in the order they are read: what happened, where that leaves us,
 *  who owes what, and what the state is owed. */
const REPORTS = [
  { path: "pl", label: () => strings.financeReportPl },
  { path: "balance", label: () => strings.financeReportBalance },
  { path: "aged", label: () => strings.financeReportAged },
  { path: "vat", label: () => strings.financeReportVat },
];

export function ReportsView() {
  return (
    <div className={styles.section}>
      <nav className={styles.tabs}>
        {REPORTS.map((report) => (
          <NavLink
            key={report.path}
            to={`${REPORTS_ROOT}/${report.path}`}
            className={({ isActive }) =>
              isActive ? `${styles.tab} ${styles.tabActive}` : styles.tab
            }
          >
            {report.label()}
          </NavLink>
        ))}
      </nav>

      <Routes>
        <Route index element={<Navigate to={`${REPORTS_ROOT}/pl`} replace />} />
        <Route path="pl" element={<PlReportView />} />
        <Route path="balance" element={<BalanceSheetView />} />
        <Route path="aged" element={<AgedReportView />} />
        <Route path="vat" element={<VatReturnView />} />
        {/* An unknown report is a stale link, not an error page. */}
        <Route path="*" element={<Navigate to={`${REPORTS_ROOT}/pl`} replace />} />
      </Routes>
    </div>
  );
}
