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
    <div className="flex min-h-0 flex-col gap-4 overflow-auto px-5 py-4">
      <form
        className="flex flex-wrap items-center gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          setPeriod(form);
        }}
      >
        <label className="inline-flex items-center gap-2 text-sm text-secondary">
          {strings.projectsReportFrom}
          <input
            className="rounded-md border border-default bg-surface px-2.5 py-1.5 text-sm text-primary focus-visible:outline-2 focus-visible:outline-accent"
            type="date"
            value={form.from}
            onChange={(e) => setForm({ ...form, from: e.target.value })}
            required
          />
        </label>
        <label className="inline-flex items-center gap-2 text-sm text-secondary">
          {strings.projectsReportTo}
          <input
            className="rounded-md border border-default bg-surface px-2.5 py-1.5 text-sm text-primary focus-visible:outline-2 focus-visible:outline-accent"
            type="date"
            value={form.to}
            onChange={(e) => setForm({ ...form, to: e.target.value })}
            required
          />
        </label>
        <Button type="submit">{strings.projectsReportShow}</Button>
        <button
          type="button"
          className="p-0 text-sm text-link"
          onClick={() => pick(quarterOf(new Date()))}
        >
          {strings.projectsReportThisQuarter}
        </button>
        <button
          type="button"
          className="p-0 text-sm text-link"
          onClick={() => pick(previousQuarterOf(new Date()))}
        >
          {strings.projectsReportLastQuarter}
        </button>
        {(loading || downloading) && <Spinner size={16} />}
        <span className="flex-1" />
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
        <p className="m-0 text-xs text-tertiary">
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
    <div className="overflow-x-auto rounded-lg border border-subtle bg-surface">
      <table className="w-full border-collapse text-sm [&_th]:whitespace-nowrap [&_th]:border-b [&_th]:border-subtle [&_th]:px-3.5 [&_th]:py-2.5 [&_th]:text-left [&_th]:font-medium [&_th]:text-tertiary [&_td]:border-b [&_td]:border-subtle [&_td]:px-3.5 [&_td]:py-2.5 [&_td]:align-middle [&_td]:text-primary [&_tbody_tr:hover]:bg-raised">
        <thead>
          <tr>
            <th scope="col">{strings.projectsProject}</th>
            <th scope="col">{strings.projectsCustomer}</th>
            <th scope="col" className="whitespace-nowrap text-right tabular-nums">
              {strings.projectsHoursLogged}
            </th>
            <th scope="col" className="whitespace-nowrap text-right tabular-nums">
              {strings.projectsBillableHours}
            </th>
            <th scope="col" className="whitespace-nowrap text-right tabular-nums">
              {strings.projectsReportColValue}
            </th>
            <th scope="col" className="whitespace-nowrap text-right tabular-nums">
              {strings.projectsReportColInvoiced}
            </th>
            <th scope="col" className="whitespace-nowrap text-right tabular-nums">
              {strings.projectsReportColToInvoice}
            </th>
            <th scope="col" className="whitespace-nowrap text-right tabular-nums">
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
            <td className="whitespace-nowrap text-right tabular-nums">{durationLabel(report.totals.minutes)}</td>
            <td className="whitespace-nowrap text-right tabular-nums">{durationLabel(report.totals.billableMinutes)}</td>
            <td className="whitespace-nowrap text-right tabular-nums">
              <Money rows={report.totals.byCurrency} pick={(row) => row.netCents} locale={locale} />
            </td>
            <td className="whitespace-nowrap text-right tabular-nums">
              <Money
                rows={report.totals.byCurrency}
                pick={(row) => row.billedNetCents}
                locale={locale}
              />
            </td>
            <td className="whitespace-nowrap text-right tabular-nums">
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
        <span className="font-medium">{project.projectName}</span>
        {/* Chargeable hours nobody has priced are named where they were
            worked, because that is where somebody can do something about
            them — never folded into a value of zero. */}
        {project.unratedMinutes > 0 && (
          <span className="block text-xs text-tertiary" title={strings.projectsReportUnratedHint}>
            {strings.projectsReportUnrated(durationLabel(project.unratedMinutes))}
          </span>
        )}
      </td>
      <td className={customer === null ? "italic text-tertiary" : undefined}>
        {customer ?? strings.projectsCustomerUnknown}
      </td>
      <td className="whitespace-nowrap text-right tabular-nums">{durationLabel(project.minutes)}</td>
      <td className="whitespace-nowrap text-right tabular-nums">{durationLabel(project.billableMinutes)}</td>
      <td className="whitespace-nowrap text-right tabular-nums">
        <Money rows={project.byCurrency} pick={(row) => row.netCents} locale={locale} />
      </td>
      <td className="whitespace-nowrap text-right tabular-nums">
        <Money rows={project.byCurrency} pick={(row) => row.billedNetCents} locale={locale} />
      </td>
      <td className="whitespace-nowrap text-right tabular-nums">
        <Money rows={project.byCurrency} pick={(row) => row.unbilledNetCents} locale={locale} />
      </td>
      <td className="whitespace-nowrap text-right tabular-nums">{durationLabel(project.toDateMinutes)}</td>
      <td>
        <BudgetBar
          consumptionBp={project.budgetConsumptionBp ?? project.hoursConsumptionBp}
          label={strings.projectsReportColBudget}
        />
        <span className="block text-xs text-tertiary">
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
  if (rows.length === 0) return <span className="text-tertiary">{strings.projectsReportNoValue}</span>;
  return (
    <>
      {rows.map((row) => (
        <span key={row.currency} className="block tabular-nums">
          {formatAmount(pick(row), locale, row.currency)}
        </span>
      ))}
    </>
  );
}
