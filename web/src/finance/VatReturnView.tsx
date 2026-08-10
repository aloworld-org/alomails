// The VAT-return figures: what was charged, what was paid, and the one number a
// return form asks for.
//
// **These are the journal's figures, not the invoices'.** The billing module has
// a VAT summary of its own (B1.20) and neither replaces the other: that one
// shows what was invoiced, per currency, with the counts behind it; this one
// shows what the books carry — including the purchase side no invoice of ours
// knows about — and it is the one a return is filed from. Both are real, and the
// suite asserts they agree on the sales side.
//
// **The net is said in words as well as in figures.** Positive means the tenant
// owes the authority and negative means it is owed a refund; a signed number in
// a cell is a thing people misread once a quarter, and the sentence above it is
// what stops that.
//
// **Nothing here is a rate the browser computed.** Basis points arrive from the
// server and are printed by Billing's formatter, the same one an invoice and its
// PDF use, so a rate never reads two ways in one tenant's paperwork.
import { useCallback, useEffect, useState } from "react";
import { Receipt } from "lucide-react";

import { formatRate, previousQuarterOf, quarterOf, type Period } from "../billing";
import { getLocale, strings } from "../i18n";
import { financeMessage, useFinanceApi } from "./api";
import { amountLabel } from "./format";
import { EmptyState, ErrorBanner } from "./parts";
import { PeriodToolbar, ReportBasis, useCsvDownload } from "./reportParts";
import type { VatReturn, VatReturnSide } from "./types";
import styles from "./FinanceModule.module.css";

export function VatReturnView() {
  const api = useFinanceApi();
  // The quarter that is being *declared* is the one that has ended — which is
  // why this screen opens on it and not on the one being lived through.
  const [period, setPeriod] = useState<Period>(() => previousQuarterOf(new Date()));
  const [form, setForm] = useState<Period>(period);
  const [report, setReport] = useState<VatReturn | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const csv = useCsvDownload();

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setReport(await api.vatReturn(period.from, period.to));
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
  const empty =
    report !== null &&
    report.output.rates.length === 0 &&
    report.input.rates.length === 0 &&
    report.netPayableCents === 0;

  return (
    <div className={styles.page}>
      <PeriodToolbar
        form={form}
        picks={[
          { label: strings.financeReportLastQuarter, period: previousQuarterOf(today) },
          { label: strings.financeReportThisQuarter, period: quarterOf(today) },
        ]}
        busy={loading || csv.downloading}
        canDownload={report !== null}
        onForm={setForm}
        onApply={setPeriod}
        onDownload={() =>
          csv.download(
            () => api.vatReturnCsv(period.from, period.to),
            `vat-return-${period.from}-to-${period.to}.csv`,
          )
        }
      />

      <ReportBasis from={period.from} to={period.to} />
      {error !== null && <ErrorBanner message={error} />}
      {csv.error !== null && <ErrorBanner message={csv.error} />}

      {empty && !loading && (
        <EmptyState
          Icon={Receipt}
          title={strings.financeReportEmptyTitle}
          body={strings.financeReportEmptyBody}
        />
      )}

      {report !== null && !empty && (
        <>
          <div className={styles.tableWrap}>
            <table className={styles.table}>
              <caption className={styles.srOnly}>{strings.financeReportVat}</caption>
              <thead>
                <tr>
                  <th scope="col">{strings.financeReportVatRate}</th>
                  <th scope="col" className={styles.numeric}>
                    {strings.financeReportVatBase}
                  </th>
                  <th scope="col" className={styles.numeric}>
                    {strings.financeReportVatTax}
                  </th>
                </tr>
              </thead>
              <tbody>
                <Side
                  title={strings.financeReportVatOutput}
                  side={report.output}
                  currency={report.currency}
                  totalLabel={strings.financeReportVatOutputTotal}
                />
                <Side
                  title={strings.financeReportVatInput}
                  side={report.input}
                  currency={report.currency}
                  totalLabel={strings.financeReportVatInputTotal}
                />
              </tbody>
              <tfoot>
                <tr>
                  <th scope="row" colSpan={2}>
                    {report.netPayableCents < 0
                      ? strings.financeReportVatRefund
                      : strings.financeReportVatPayable}
                  </th>
                  <td className={styles.numeric}>
                    {amountLabel(report.netPayableCents, report.currency)}
                  </td>
                </tr>
              </tfoot>
            </table>
          </div>
          <p className={styles.sectionNote}>{strings.financeReportVatNote}</p>
        </>
      )}
    </div>
  );
}

/** One side of the return: its heading, a row per rate, the turnover on no rate
 *  at all, and its total.
 *
 *  The unrated row is drawn even when it is zero on purpose: its absence would
 *  read as "the question does not arise" when what it means is "the answer is
 *  none". */
function Side({
  title,
  side,
  currency,
  totalLabel,
}: {
  title: string;
  side: VatReturnSide;
  currency: string;
  totalLabel: string;
}) {
  return (
    <>
      <tr>
        <th scope="colgroup" colSpan={3} className={styles.sectionTitle}>
          {title}
        </th>
      </tr>
      {side.rates.map((rate) => (
        <tr key={rate.rateBp}>
          <td>{formatRate(rate.rateBp, getLocale())}</td>
          <td className={styles.numeric}>{amountLabel(rate.baseCents, currency)}</td>
          <td className={styles.numeric}>{amountLabel(rate.vatCents, currency)}</td>
        </tr>
      ))}
      <tr>
        <td className={styles.muted}>{strings.financeReportVatUnrated}</td>
        <td className={styles.numeric}>{amountLabel(side.unratedBaseCents, currency)}</td>
        <td className={styles.numeric}>{amountLabel(side.unratedVatCents, currency)}</td>
      </tr>
      <tr>
        <th scope="row">{totalLabel}</th>
        <td className={styles.numeric}>{amountLabel(side.baseCents, currency)}</td>
        <td className={styles.numeric}>{amountLabel(side.vatCents, currency)}</td>
      </tr>
    </>
  );
}
