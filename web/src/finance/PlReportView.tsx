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
import { Table, Td, Th } from "../ds";
import { strings } from "../i18n";
import { financeMessage, useFinanceApi } from "./api";
import { amountLabel, dayLabel } from "./format";
import { EmptyState, ErrorBanner } from "./parts";
import {
  PeriodToolbar,
  ReportBasis,
  SectionHeading,
  useCsvDownload,
} from "./reportParts";
import type { PlLine, PlReport } from "./types";
import { LedgerDialog } from "./LedgerDialog";
import styles from "./FinanceModule.module.css";

export function PlReportView() {
  const api = useFinanceApi();
  const [period, setPeriod] = useState<Period>(() => yearOf(new Date()));
  const [form, setForm] = useState<Period>(period);
  const [report, setReport] = useState<PlReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const csv = useCsvDownload();
  const [ledgerLine,setLedgerLine]=useState<PlLine|null>(null);

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
          {
            label: strings.financeReportLastQuarter,
            period: previousQuarterOf(today),
          },
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

      {report !== null &&
        (report.income.length > 0 || report.expense.length > 0) && (
          // The caption is `Table`'s own — read always, drawn only on the two
          // report tables whose heading does not already say which they are.
          <Table label={strings.financeReportPl}>
            <thead>
              <tr>
                <Th>{strings.financeAccountCode}</Th>
                <Th>{strings.financeAccountName}</Th>
                <Th numeric>{strings.financeReportAmount}</Th>
                {/* The comparative the SERVER chose, named by its own days: a
                  column headed "previous" that does not say which days it
                  covers is a figure nobody can check. */}
                <Th numeric>
                  {strings.financeReportPrevious(
                    dayLabel(report.previousFrom, report.previousFrom),
                    dayLabel(report.previousTo, report.previousTo),
                  )}
                </Th>
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
                onOpen={setLedgerLine}
              />
              <Section
                title={strings.financeReportExpense}
                lines={report.expense}
                currency={report.currency}
                totalCents={report.expenseCents}
                previousCents={report.previousExpenseCents}
                totalLabel={strings.financeReportExpenseTotal}
                onOpen={setLedgerLine}
              />
            </tbody>
            <tfoot>
              <tr>
                <Th scope="row" colSpan={2}>
                  {report.resultCents < 0
                    ? strings.financeReportLoss
                    : strings.financeReportProfit}
                </Th>
                <Td numeric>
                  {amountLabel(report.resultCents, report.currency)}
                </Td>
                <Td numeric>
                  {amountLabel(report.previousResultCents, report.currency)}
                </Td>
              </tr>
            </tfoot>
      </Table>
        )}
      {ledgerLine&&report&&<LedgerDialog accountId={ledgerLine.accountId} name={ledgerLine.name} currency={report.currency} from={period.from} to={period.to} onClose={()=>setLedgerLine(null)}/>}
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
  onOpen,
}: {
  title: string;
  lines: PlLine[];
  currency: string;
  totalCents: number;
  previousCents: number;
  totalLabel: string;
  onOpen: (line: PlLine) => void;
}) {
  return (
    <>
      <SectionHeading title={title} cols={4} />
      {lines.map((line) => (
        <tr key={line.accountId}>
          <Td>{line.code}</Td>
          <Td><button type="button" className="font-medium text-primary underline decoration-subtle underline-offset-4 hover:text-accent" onClick={()=>onOpen(line)}>{line.name}</button></Td>
          <Td numeric>{amountLabel(line.amountCents, currency)}</Td>
          <Td numeric>{amountLabel(line.previousCents, currency)}</Td>
        </tr>
      ))}
      <tr>
        <Th scope="row" colSpan={2}>
          {totalLabel}
        </Th>
        <Td numeric>{amountLabel(totalCents, currency)}</Td>
        <Td numeric>{amountLabel(previousCents, currency)}</Td>
      </tr>
    </>
  );
}
