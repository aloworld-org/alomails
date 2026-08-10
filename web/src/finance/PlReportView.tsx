// The profit and loss: what the business earned and spent between two days.
//
// Two things it deliberately does not do. It does not compute money — every
// figure, both totals and the result, is the server's fold of the journal in
// integer cents, and there is no column here the browser adds up. And it does
// not choose the comparative period: the server states which days it compared
// with, and the column header says so, because "last year" is a claim about a
// fiscal calendar and a browser guessing at one would print a heading nobody
// chose.
import { useCallback, useEffect, useState } from "react";
import { TrendingUp } from "lucide-react";

import { previousQuarterOf, quarterOf, yearOf, type Period } from "../billing";
import { strings } from "../i18n";
import { financeMessage, useFinanceApi } from "./api";
import { amountLabel, dayLabel } from "./format";
import { EmptyState, ErrorBanner } from "./parts";
import { PeriodToolbar, ReportBasis, useCsvDownload } from "./reportParts";
import type { PlLine, PlReport } from "./types";
import styles from "./FinanceModule.module.css";

export function PlReportView() {
  const api = useFinanceApi();
  const [period, setPeriod] = useState<Period>(() => yearOf(new Date()));
  const [form, setForm] = useState<Period>(period);
  const [report, setReport] = useState<PlReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const csv = useCsvDownload();

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setReport(await api.plReport(period.from, period.to));
      setError(null);
    } catch (err) {
      setError(financeMessage(err, strings.financeReportLoadFailed));
      setReport(null);
    } finally {
      setLoading(false);
    }
  }, [api, period]);

  useEffect(() => {
    void load();
  }, [load]);

  const today = new Date();

  return (
    <div className={styles.page}>
      <PeriodToolbar
        form={form}
        picks={[
          { label: strings.financeReportThisYear, period: yearOf(today) },
          { label: strings.financeReportThisQuarter, period: quarterOf(today) },
          { label: strings.financeReportLastQuarter, period: previousQuarterOf(today) },
        ]}
        busy={loading || csv.downloading}
        canDownload={report !== null}
        onForm={setForm}
        onApply={setPeriod}
        onDownload={() =>
          csv.download(
            () => api.plReportCsv(period.from, period.to),
            `profit-and-loss-${period.from}-to-${period.to}.csv`,
          )
        }
      />

      <ReportBasis from={period.from} to={period.to} />
      {error !== null && <ErrorBanner message={error} />}
      {csv.error !== null && <ErrorBanner message={csv.error} />}

      {report !== null &&
        report.income.length === 0 &&
        report.expense.length === 0 &&
        !loading && (
          <EmptyState
            Icon={TrendingUp}
            title={strings.financeReportEmptyTitle}
            body={strings.financeReportEmptyBody}
          />
        )}

      {report !== null && (report.income.length > 0 || report.expense.length > 0) && (
        <div className={styles.tableWrap}>
          <table className={styles.table}>
            <caption className={styles.srOnly}>{strings.financeReportPl}</caption>
            <thead>
              <tr>
                <th scope="col">{strings.financeAccountCode}</th>
                <th scope="col">{strings.financeAccountName}</th>
                <th scope="col" className={styles.numeric}>
                  {strings.financeReportAmount}
                </th>
                {/* The comparative the SERVER chose, named by its own days: a
                    column headed "previous" that does not say which days it
                    covers is a figure nobody can check. */}
                <th scope="col" className={styles.numeric}>
                  {strings.financeReportPrevious(
                    dayLabel(report.previousFrom, report.previousFrom),
                    dayLabel(report.previousTo, report.previousTo),
                  )}
                </th>
              </tr>
            </thead>
            <tbody>
              <Section
                title={strings.financeReportIncome}
                lines={report.income}
                currency={report.currency}
                totalCents={report.incomeCents}
                previousCents={report.previousIncomeCents}
                totalLabel={strings.financeReportIncomeTotal}
              />
              <Section
                title={strings.financeReportExpense}
                lines={report.expense}
                currency={report.currency}
                totalCents={report.expenseCents}
                previousCents={report.previousExpenseCents}
                totalLabel={strings.financeReportExpenseTotal}
              />
            </tbody>
            <tfoot>
              <tr>
                <th scope="row" colSpan={2}>
                  {report.resultCents < 0
                    ? strings.financeReportLoss
                    : strings.financeReportProfit}
                </th>
                <td className={styles.numeric}>
                  {amountLabel(report.resultCents, report.currency)}
                </td>
                <td className={styles.numeric}>
                  {amountLabel(report.previousResultCents, report.currency)}
                </td>
              </tr>
            </tfoot>
          </table>
        </div>
      )}
    </div>
  );
}

/** One side of the result — its heading, its account lines, its total. Rendered
 *  as rows of the one table rather than as a table of its own, so the two sides
 *  share a column width and read as one document. */
function Section({
  title,
  lines,
  currency,
  totalCents,
  previousCents,
  totalLabel,
}: {
  title: string;
  lines: PlLine[];
  currency: string;
  totalCents: number;
  previousCents: number;
  totalLabel: string;
}) {
  return (
    <>
      <tr>
        <th scope="colgroup" colSpan={4} className={styles.sectionTitle}>
          {title}
        </th>
      </tr>
      {lines.map((line) => (
        <tr key={line.accountId}>
          <td>{line.code}</td>
          <td>{line.name}</td>
          <td className={styles.numeric}>{amountLabel(line.amountCents, currency)}</td>
          <td className={styles.numeric}>{amountLabel(line.previousCents, currency)}</td>
        </tr>
      ))}
      <tr>
        <th scope="row" colSpan={2}>
          {totalLabel}
        </th>
        <td className={styles.numeric}>{amountLabel(totalCents, currency)}</td>
        <td className={styles.numeric}>{amountLabel(previousCents, currency)}</td>
      </tr>
    </>
  );
}
