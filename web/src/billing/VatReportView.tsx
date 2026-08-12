// The VAT summary of a period: what was billed at each rate between two days,
// which is what a VAT return is copied from.
//
// Three things it deliberately does not do. It does not compute money — every
// figure shown is the server's, in integer cents, and there is no column here
// that the browser adds up. It does not decide what a period contains: which
// documents count (issued and paid, judged on the issue date frozen on them,
// credit notes subtracting) is the server's rule, stated once in
// `docs/design/billing.md`. And it does not add currencies together: the API
// answers one group per currency, and this page renders one table each.
//
// What it does render once, at the end, is the period **in the currency the
// tenant keeps books in** (B1.21) — the figure a return is actually filed from,
// with every document converted server-side at the rate frozen on it when it was
// issued. Where any document could not be converted, the count of those is said
// out loud above the table: a tax total that is quietly missing a document is
// worse than no total at all.
//
// The period is applied on submit rather than on every keystroke, so a
// half-typed date never becomes a request — and so the figures on screen always
// belong to the days written above them.
import { useCallback, useEffect, useState } from "react";
import { FileSpreadsheet } from "lucide-react";

import { Button, Spinner } from "../ds";
import { strings, useLocale } from "../i18n";
import { billingMessage, useBillingApi } from "./api";
import { formatDocumentDate } from "./dates";
import { saveTextFile } from "../platform/download";
import { formatAmount, formatRate } from "./money";
import { BillingLoading, EmptyState, ErrorBanner } from "./parts";
import { previousQuarterOf, quarterOf, type Period } from "./period";
import type { VatReport } from "./types";
import styles from "./BillingModule.module.css";

/** What a saved summary is called and what it is: the server names the file in
 *  its own `Content-Disposition`, and this is the same name for the copy the
 *  browser writes from memory (the route is authenticated, so the file is
 *  fetched rather than linked). */
function fileName(period: Period): string {
  return `vat-${period.from}-to-${period.to}.csv`;
}

export function VatReportView() {
  const api = useBillingApi();
  const locale = useLocale();
  // The form opens on the quarter that is being lived through; the quarter
  // that is being *declared* is one click away.
  const [period, setPeriod] = useState<Period>(() => quarterOf(new Date()));
  const [form, setForm] = useState<Period>(period);
  const [report, setReport] = useState<VatReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [downloading, setDownloading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setReport(await api.vatReport(period.from, period.to));
      setError(null);
    } catch (err) {
      // The server's own sentence when it sent one — it names the rule that
      // was broken ("to must be a date of the form YYYY-MM-DD").
      setError(billingMessage(err, strings.billingLoadFailed));
      setReport(null);
    } finally {
      setLoading(false);
    }
  }, [api, period]);

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
      const csv = await api.vatReportCsv(period.from, period.to);
      saveTextFile(csv, fileName(period), "text/csv;charset=utf-8");
      setError(null);
    } catch (err) {
      setError(billingMessage(err, strings.billingReportDownloadFailed));
    } finally {
      setDownloading(false);
    }
  }

  const readable = (day: string) => formatDocumentDate(day, locale, day);

  return (
    <div className={styles.page}>
      <form
        className={styles.toolbar}
        onSubmit={(e) => {
          e.preventDefault();
          setPeriod(form);
        }}
      >
        <label className={styles.toggle}>
          {strings.billingReportFrom}
          <input
            className={styles.input}
            type="date"
            value={form.from}
            onChange={(e) => setForm({ ...form, from: e.target.value })}
            required
          />
        </label>
        <label className={styles.toggle}>
          {strings.billingReportTo}
          <input
            className={styles.input}
            type="date"
            value={form.to}
            onChange={(e) => setForm({ ...form, to: e.target.value })}
            required
          />
        </label>
        <Button type="submit">{strings.billingReportShow}</Button>
        <button
          type="button"
          className={styles.linkAction}
          onClick={() => pick(quarterOf(new Date()))}
        >
          {strings.billingReportThisQuarter}
        </button>
        <button
          type="button"
          className={styles.linkAction}
          onClick={() => pick(previousQuarterOf(new Date()))}
        >
          {strings.billingReportLastQuarter}
        </button>
        {(loading || downloading) && <Spinner size={16} />}
        <Button
          variant="ghost"
          onClick={() => void download()}
          disabled={report === null || downloading}
        >
          {strings.billingReportDownloadCsv}
        </Button>
      </form>

      <p className={styles.totalsNote}>
        {strings.billingReportBasis(readable(period.from), readable(period.to))}
      </p>

      {error !== null && <ErrorBanner message={error} />}

      {loading ? <BillingLoading /> : report !== null && report.currencies.length === 0 ? (
        <EmptyState
          Icon={FileSpreadsheet}
          title={strings.billingReportEmptyTitle}
          body={strings.billingReportEmptyBody}
        />
      ) : (
        report?.currencies.map((group) => (
          <section key={group.currency} className={styles.tableWrap}>
            <table className={styles.table}>
              <caption className={styles.srOnly}>
                {strings.billingReportCaption(group.currency)}
              </caption>
              <thead>
                <tr>
                  <th scope="col">{strings.billingColVatRate}</th>
                  <th scope="col" className={styles.numeric}>
                    {strings.billingColNet}
                  </th>
                  <th scope="col" className={styles.numeric}>
                    {strings.billingReportColVat}
                  </th>
                </tr>
              </thead>
              <tbody>
                {group.byRate.map((rate) => (
                  <tr key={rate.rateBp}>
                    <td>{formatRate(rate.rateBp, locale)}</td>
                    <td className={styles.numeric}>
                      {formatAmount(rate.netCents, locale, group.currency)}
                    </td>
                    <td className={styles.numeric}>
                      {formatAmount(rate.vatCents, locale, group.currency)}
                    </td>
                  </tr>
                ))}
              </tbody>
              <tfoot>
                <tr>
                  <th scope="row">{strings.billingReportTotal}</th>
                  <td className={styles.numeric}>
                    {formatAmount(group.netCents, locale, group.currency)}
                  </td>
                  <td className={styles.numeric}>
                    {formatAmount(group.vatCents, locale, group.currency)}
                  </td>
                </tr>
                <tr>
                  <th scope="row">{strings.billingReportGross}</th>
                  <td className={styles.numeric} colSpan={2}>
                    {formatAmount(group.grossCents, locale, group.currency)}
                  </td>
                </tr>
              </tfoot>
            </table>
            <p className={styles.totalsNote}>
              {strings.billingReportCounts(group.invoiceCount, group.creditNoteCount)}
            </p>
          </section>
        ))
      )}

      {/* The period in the accounting currency. Rendered only when the report
          holds something *and* it says something the tables above do not — a
          tenant that bills solely in its own currency would otherwise read the
          same figures twice. */}
      {report !== null && report.currencies.length > 0 && restatesAnything(report) && (
        <section className={styles.tableWrap}>
          <p className={styles.totalsNote}>
            {strings.billingReportBaseIntro(report.base.currency)}
          </p>
          {report.base.unconvertedCount > 0 && (
            <ErrorBanner message={strings.billingReportUnconverted(report.base.unconvertedCount)} />
          )}
          <table className={styles.table}>
            <caption className={styles.srOnly}>
              {strings.billingReportBaseCaption(report.base.currency)}
            </caption>
            <thead>
              <tr>
                <th scope="col">{strings.billingColVatRate}</th>
                <th scope="col" className={styles.numeric}>
                  {strings.billingColNet}
                </th>
                <th scope="col" className={styles.numeric}>
                  {strings.billingReportColVat}
                </th>
              </tr>
            </thead>
            <tbody>
              {report.base.byRate.map((rate) => (
                <tr key={rate.rateBp}>
                  <td>{formatRate(rate.rateBp, locale)}</td>
                  <td className={styles.numeric}>
                    {formatAmount(rate.netCents, locale, report.base.currency)}
                  </td>
                  <td className={styles.numeric}>
                    {formatAmount(rate.vatCents, locale, report.base.currency)}
                  </td>
                </tr>
              ))}
            </tbody>
            <tfoot>
              <tr>
                <th scope="row">{strings.billingReportTotal}</th>
                <td className={styles.numeric}>
                  {formatAmount(report.base.netCents, locale, report.base.currency)}
                </td>
                <td className={styles.numeric}>
                  {formatAmount(report.base.vatCents, locale, report.base.currency)}
                </td>
              </tr>
              <tr>
                <th scope="row">{strings.billingReportGross}</th>
                <td className={styles.numeric} colSpan={2}>
                  {formatAmount(report.base.grossCents, locale, report.base.currency)}
                </td>
              </tr>
            </tfoot>
          </table>
        </section>
      )}
    </div>
  );
}

/** Whether the accounting-currency table would say anything the per-currency
 *  tables above do not: a second currency was billed, or something in the period
 *  could not be converted at all. A single-currency tenant sees one table, not
 *  the same figures twice. */
function restatesAnything(report: VatReport): boolean {
  return (
    report.base.unconvertedCount > 0 ||
    report.currencies.some((group) => group.currency !== report.base.currency)
  );
}
