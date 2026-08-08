// Profitability per engagement (alo Projects, B3.08) — the fourth tab, and the
// only screen in Projects that shows money adding up.
//
// Three things it deliberately does not do. It does not compute money: every
// amount is the server's integer cents, folded there from the very rows a
// billing line carries, and there is no column here the browser adds up. It
// does not add currencies together — the API answers one row per currency and
// converts nothing, so an engagement priced in two shows two lines. And it says
// *value*, never *margin*: this is the revenue side, and what an hour costs is
// the ledger's answer, which does not exist yet.
//
// What it does say out loud is that the two halves are dated differently: the
// hours are the period's, and the budget is consumed by everything up to the
// period's last day. A screen that put both under one date range would report
// an engagement as 5% spent forever.
//
// The period is applied on submit rather than on every keystroke, so a
// half-typed date never becomes a request.
import { useCallback, useEffect, useState } from "react";
import { PieChart } from "lucide-react";

import { formatAmount, previousQuarterOf, quarterOf, type Period } from "../billing";
import { Button, Spinner } from "../ds";
import { strings, useLocale } from "../i18n";
import { saveTextFile } from "../platform/download";
import { projectsMessage, useProjectsApi } from "./api";
import { dayLabel, durationLabel } from "./format";
import { BudgetBar, EmptyState, ErrorBanner } from "./parts";
import type { ProfitabilityCurrency, ProfitabilityReport, ProjectProfitability } from "./types";
import styles from "./ProjectsModule.module.css";

interface Props {
  /** A customer's own name for an id, or `null` when this reader cannot see
   *  them — the row says "unknown" rather than printing a raw id at somebody. */
  customerName: (customerId: string) => string | null;
  /** Bumped when an hour was written anywhere, so the figures follow the work. */
  revision: number;
}

/** What a saved report is called: the server names the file in its own
 *  `Content-Disposition`, and this is the same name for the copy the browser
 *  writes from memory (the route is authenticated, so the file is fetched
 *  rather than linked). */
function fileName(period: Period): string {
  return `profitability-${period.from}-to-${period.to}.csv`;
}

export function ReportView({ customerName, revision }: Props) {
  const api = useProjectsApi();
  // The form opens on the quarter being lived through; the one being reviewed
  // is one click away.
  const [period, setPeriod] = useState<Period>(() => quarterOf(new Date()));
  const [form, setForm] = useState<Period>(period);
  const [report, setReport] = useState<ProfitabilityReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [downloading, setDownloading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setReport(await api.profitability(period.from, period.to));
      setError(null);
    } catch (err) {
      // The server's own sentence when it sent one — it names the rule that was
      // broken ("the period ends before it starts").
      setError(projectsMessage(err, strings.projectsLoadFailed));
      setReport(null);
    } finally {
      setLoading(false);
    }
  }, [api, period, revision]);

  useEffect(() => {
    void load();
  }, [load]);

  /** Applies a period from a quick pick: the form and the request move
   *  together, so what is shown always matches what is written. */
  function pick(next: Period) {
    setForm(next);
    setPeriod(next);
  }

  async function download() {
    setDownloading(true);
    try {
      const csv = await api.profitabilityCsv(period.from, period.to);
      saveTextFile(csv, fileName(period), "text/csv;charset=utf-8");
      setError(null);
    } catch (err) {
      setError(projectsMessage(err, strings.projectsReportDownloadFailed));
    } finally {
      setDownloading(false);
    }
  }

  return (
    <div className={styles.page}>
      <form
        className={styles.toolbar}
        onSubmit={(e) => {
          e.preventDefault();
          setPeriod(form);
        }}
      >
        <label className={styles.periodField}>
          {strings.projectsReportFrom}
          <input
            className={styles.periodInput}
            type="date"
            value={form.from}
            onChange={(e) => setForm({ ...form, from: e.target.value })}
            required
          />
        </label>
        <label className={styles.periodField}>
          {strings.projectsReportTo}
          <input
            className={styles.periodInput}
            type="date"
            value={form.to}
            onChange={(e) => setForm({ ...form, to: e.target.value })}
            required
          />
        </label>
        <Button type="submit">{strings.projectsReportShow}</Button>
        <button
          type="button"
          className={styles.linkAction}
          onClick={() => pick(quarterOf(new Date()))}
        >
          {strings.projectsReportThisQuarter}
        </button>
        <button
          type="button"
          className={styles.linkAction}
          onClick={() => pick(previousQuarterOf(new Date()))}
        >
          {strings.projectsReportLastQuarter}
        </button>
        {(loading || downloading) && <Spinner size={16} />}
        <span className={styles.toolbarSpacer} />
        <Button
          variant="ghost"
          onClick={() => void download()}
          disabled={report === null || downloading}
        >
          {strings.projectsReportDownloadCsv}
        </Button>
      </form>

      {error !== null && <ErrorBanner message={error} />}

      {report !== null && report.projects.length === 0 && !loading ? (
        <EmptyState
          Icon={PieChart}
          title={strings.projectsReportEmptyTitle}
          body={strings.projectsReportEmptyBody}
        />
      ) : (
        report !== null && <ReportTable report={report} customerName={customerName} />
      )}

      {report !== null && report.projects.length > 0 && (
        <p className={styles.reportBasis}>
          {strings.projectsReportBasis(dayLabel(report.from), dayLabel(report.to))}{" "}
          {strings.projectsReportBudgetBasis(dayLabel(report.to))}
        </p>
      )}
    </div>
  );
}

/** One engagement per row, with the whole report's totals under them. */
function ReportTable({
  report,
  customerName,
}: {
  report: ProfitabilityReport;
  customerName: (customerId: string) => string | null;
}) {
  const locale = useLocale();
  return (
    <div className={styles.tableWrap}>
      <table className={styles.table}>
        <thead>
          <tr>
            <th scope="col">{strings.projectsProject}</th>
            <th scope="col">{strings.projectsCustomer}</th>
            <th scope="col" className={styles.numeric}>
              {strings.projectsHoursLogged}
            </th>
            <th scope="col" className={styles.numeric}>
              {strings.projectsBillableHours}
            </th>
            <th scope="col" className={styles.numeric}>
              {strings.projectsReportColValue}
            </th>
            <th scope="col" className={styles.numeric}>
              {strings.projectsReportColInvoiced}
            </th>
            <th scope="col" className={styles.numeric}>
              {strings.projectsReportColToInvoice}
            </th>
            <th scope="col" className={styles.numeric}>
              {strings.projectsReportColToDate}
            </th>
            <th scope="col">{strings.projectsReportColBudget}</th>
          </tr>
        </thead>
        <tbody>
          {report.projects.map((project) => (
            <ProjectRow
              key={project.projectId}
              project={project}
              customerName={customerName}
              locale={locale}
            />
          ))}
        </tbody>
        <tfoot>
          <tr>
            <th scope="row" colSpan={2}>
              {strings.projectsReportTotals}
            </th>
            <td className={styles.numeric}>{durationLabel(report.totals.minutes)}</td>
            <td className={styles.numeric}>{durationLabel(report.totals.billableMinutes)}</td>
            <td className={styles.numeric}>
              <Money rows={report.totals.byCurrency} pick={(row) => row.netCents} locale={locale} />
            </td>
            <td className={styles.numeric}>
              <Money
                rows={report.totals.byCurrency}
                pick={(row) => row.billedNetCents}
                locale={locale}
              />
            </td>
            <td className={styles.numeric}>
              <Money
                rows={report.totals.byCurrency}
                pick={(row) => row.unbilledNetCents}
                locale={locale}
              />
            </td>
            {/* No total of hours-to-date and no total of budgets: a budget
                belongs to an engagement, and a sum of them is a plan nobody
                made. */}
            <td />
            <td />
          </tr>
        </tfoot>
      </table>
    </div>
  );
}

function ProjectRow({
  project,
  customerName,
  locale,
}: {
  project: ProjectProfitability;
  customerName: (customerId: string) => string | null;
  locale: string;
}) {
  const customer = customerName(project.customerId);
  const remaining = project.budgetRemainingCents;
  return (
    <tr>
      <td>
        <span className={styles.gridProjectName}>{project.projectName}</span>
        {/* Chargeable hours nobody has priced are named where they were
            worked, because that is where somebody can do something about
            them — never folded into a value of zero. */}
        {project.unratedMinutes > 0 && (
          <span className={styles.subtle} title={strings.projectsReportUnratedHint}>
            {strings.projectsReportUnrated(durationLabel(project.unratedMinutes))}
          </span>
        )}
      </td>
      <td className={customer === null ? styles.internal : undefined}>
        {customer ?? strings.projectsCustomerUnknown}
      </td>
      <td className={styles.numeric}>{durationLabel(project.minutes)}</td>
      <td className={styles.numeric}>{durationLabel(project.billableMinutes)}</td>
      <td className={styles.numeric}>
        <Money rows={project.byCurrency} pick={(row) => row.netCents} locale={locale} />
      </td>
      <td className={styles.numeric}>
        <Money rows={project.byCurrency} pick={(row) => row.billedNetCents} locale={locale} />
      </td>
      <td className={styles.numeric}>
        <Money rows={project.byCurrency} pick={(row) => row.unbilledNetCents} locale={locale} />
      </td>
      <td className={styles.numeric}>{durationLabel(project.toDateMinutes)}</td>
      <td>
        <BudgetBar
          consumptionBp={project.budgetConsumptionBp ?? project.hoursConsumptionBp}
          label={strings.projectsReportColBudget}
        />
        <span className={styles.subtle}>
          {remaining === null
            ? strings.projectsReportNoBudget
            : remaining < 0
              ? strings.projectsReportBudgetOver(
                  formatAmount(-remaining, locale, project.currency),
                )
              : strings.projectsReportBudgetLeft(
                  formatAmount(remaining, locale, project.currency),
                )}
        </span>
      </td>
    </tr>
  );
}

/** An amount per currency, one line each — never a sum across them, which is
 *  why this is a list and not a number. Nothing at all reads as "no value yet",
 *  because an engagement whose hours are unpriced has not earned zero. */
function Money({
  rows,
  pick,
  locale,
}: {
  rows: ProfitabilityCurrency[];
  pick: (row: ProfitabilityCurrency) => number;
  locale: string;
}) {
  if (rows.length === 0) return <span className={styles.muted}>{strings.projectsReportNoValue}</span>;
  return (
    <>
      {rows.map((row) => (
        <span key={row.currency} className={styles.moneyLine}>
          {formatAmount(pick(row), locale, row.currency)}
        </span>
      ))}
    </>
  );
}
