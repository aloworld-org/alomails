// The balance sheet: what the business owns, owes and is worth on one day.
//
// **One date, not a period.** A balance sheet is cumulative by definition —
// every posting on or before the day counts, back to the day the books opened —
// so this screen asks for a day and the server refuses a `from` outright.
//
// **It says out loud whether it balances.** A sheet that does not balance prints
// exactly like one that does, and rounding the difference into equity is how a
// broken set of books goes unnoticed for a year. So the server states
// `differenceCents` and `balances`, and when the difference is anything but zero
// this screen shows it as a failure rather than as a figure.
//
// **The result sits beside equity, not inside it.** alo writes no year-end
// closing entry, so income less expense to the date is its own line — which is
// what makes assets equal liabilities plus equity plus result, and is what an
// accountant expects on books nobody has closed.
import { useCallback, useEffect, useState } from "react";
import { Scale } from "lucide-react";

import { previousYearOf } from "../billing";
import { strings } from "../i18n";
import { financeMessage, useFinanceApi } from "./api";
import { amountLabel, today } from "./format";
import { EmptyState, ErrorBanner } from "./parts";
import { DayToolbar, ReportBasis, useCsvDownload } from "./reportParts";
import type { BalanceLine, BalanceSheet } from "./types";
import styles from "./FinanceModule.module.css";

export function BalanceSheetView() {
  const api = useFinanceApi();
  const [on, setOn] = useState<string>(() => today());
  const [form, setForm] = useState<string>(on);
  const [sheet, setSheet] = useState<BalanceSheet | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const csv = useCsvDownload();

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setSheet(await api.balanceSheet(on));
      setError(null);
    } catch (err) {
      setError(financeMessage(err, strings.financeReportLoadFailed));
      setSheet(null);
    } finally {
      setLoading(false);
    }
  }, [api, on]);

  useEffect(() => {
    void load();
  }, [load]);

  const empty =
    sheet !== null &&
    sheet.assets.length === 0 &&
    sheet.liabilities.length === 0 &&
    sheet.equity.length === 0;

  return (
    <div className={styles.page}>
      <DayToolbar
        form={form}
        picks={[
          { label: strings.financeReportToday, on: today() },
          { label: strings.financeReportLastYearEnd, on: previousYearOf(new Date()).to },
        ]}
        busy={loading || csv.downloading}
        canDownload={sheet !== null}
        onForm={setForm}
        onApply={setOn}
        onDownload={() =>
          csv.download(() => api.balanceSheetCsv(on), `balance-sheet-${on}.csv`)
        }
      />

      <ReportBasis from={on} />
      {error !== null && <ErrorBanner message={error} />}
      {csv.error !== null && <ErrorBanner message={csv.error} />}

      {/* The one thing a balance sheet must never do quietly. */}
      {sheet !== null && !sheet.balances && (
        <ErrorBanner
          message={strings.financeReportUnbalanced(
            amountLabel(sheet.differenceCents, sheet.currency),
          )}
        />
      )}

      {empty && !loading && (
        <EmptyState
          Icon={Scale}
          title={strings.financeReportEmptyTitle}
          body={strings.financeReportEmptyBody}
        />
      )}

      {sheet !== null && !empty && (
        <div className={styles.tableWrap}>
          <table className={styles.table}>
            <caption className={styles.srOnly}>{strings.financeReportBalance}</caption>
            <thead>
              <tr>
                <th scope="col">{strings.financeAccountCode}</th>
                <th scope="col">{strings.financeAccountName}</th>
                <th scope="col" className={styles.numeric}>
                  {strings.financeReportAmount}
                </th>
              </tr>
            </thead>
            <tbody>
              <Section
                title={strings.financeReportAssets}
                lines={sheet.assets}
                currency={sheet.currency}
                totalLabel={strings.financeReportAssetsTotal}
                totalCents={sheet.assetCents}
              />
              <Section
                title={strings.financeReportLiabilities}
                lines={sheet.liabilities}
                currency={sheet.currency}
                totalLabel={strings.financeReportLiabilitiesTotal}
                totalCents={sheet.liabilityCents}
              />
              <Section
                title={strings.financeReportEquity}
                lines={sheet.equity}
                currency={sheet.currency}
                totalLabel={strings.financeReportEquityTotal}
                totalCents={sheet.equityCents}
              />
              <tr>
                <th scope="row" colSpan={2}>
                  {strings.financeReportResultToDate}
                </th>
                <td className={styles.numeric}>
                  {amountLabel(sheet.resultCents, sheet.currency)}
                </td>
              </tr>
            </tbody>
            <tfoot>
              <tr>
                <th scope="row" colSpan={2}>
                  {strings.financeReportLiabilitiesEquityTotal}
                </th>
                <td className={styles.numeric}>
                  {amountLabel(sheet.liabilityEquityCents, sheet.currency)}
                </td>
              </tr>
              <tr>
                <th scope="row" colSpan={2}>
                  {strings.financeReportDifference}
                </th>
                <td className={styles.numeric}>
                  {amountLabel(sheet.differenceCents, sheet.currency)}
                </td>
              </tr>
            </tfoot>
          </table>
        </div>
      )}
    </div>
  );
}

/** One side of the sheet, as rows of the one table: its heading, its accounts,
 *  its total. */
function Section({
  title,
  lines,
  currency,
  totalLabel,
  totalCents,
}: {
  title: string;
  lines: BalanceLine[];
  currency: string;
  totalLabel: string;
  totalCents: number;
}) {
  return (
    <>
      <tr>
        <th scope="colgroup" colSpan={3} className={styles.sectionTitle}>
          {title}
        </th>
      </tr>
      {lines.map((line) => (
        <tr key={line.accountId}>
          <td>{line.code}</td>
          <td>{line.name}</td>
          <td className={styles.numeric}>{amountLabel(line.amountCents, currency)}</td>
        </tr>
      ))}
      <tr>
        <th scope="row" colSpan={2}>
          {totalLabel}
        </th>
        <td className={styles.numeric}>{amountLabel(totalCents, currency)}</td>
      </tr>
    </>
  );
}
