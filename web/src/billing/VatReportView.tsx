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
import { CalendarRange, Download, FileSpreadsheet } from "lucide-react";

import { Button, Spinner } from "../ds";
import { strings, useLocale } from "../i18n";
import { billingMessage, useBillingApi } from "./api";
import { formatDocumentDate } from "./dates";
import { saveTextFile } from "../platform/download";
import { formatAmount, formatRate } from "./money";
import { ErrorBanner } from "./parts";
import { previousQuarterOf, quarterOf, type Period } from "./period";
import type { VatReport } from "./types";

const dateInput =
  "h-11 min-w-0 rounded-lg border border-default bg-surface px-3 text-sm text-primary outline-none transition-colors focus:border-accent focus:ring-2 focus:ring-accent/15";
const quickAction =
  "inline-flex h-10 items-center justify-center whitespace-nowrap rounded-lg border border-default bg-surface px-4 text-sm font-medium !text-secondary !no-underline transition-colors hover:border-accent hover:bg-[var(--accent-soft)] hover:!text-accent hover:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/20";
const tableShell = "min-h-0 overflow-auto rounded-xl border border-subtle bg-surface shadow-sm";
const tableClass = "w-full border-collapse text-sm";
const headCell =
  "sticky top-0 z-[1] whitespace-nowrap border-b border-default bg-sunken px-4 py-3 text-left text-xs font-semibold text-tertiary";
const bodyCell = "border-b border-subtle px-4 py-3 text-secondary";
const numberCell = "text-right tabular-nums";

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
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-8 pb-8 pt-6 max-sm:px-4">
      <header className="mx-auto mb-6 flex w-full max-w-[90rem] items-start justify-between gap-6 max-sm:flex-col">
        <div className="flex min-w-0 items-start gap-3">
          <span className="flex size-10 shrink-0 items-center justify-center rounded-lg bg-[var(--accent-soft)] text-accent">
            <FileSpreadsheet className="size-5" aria-hidden="true" />
          </span>
          <div className="min-w-0">
            <h2 className="m-0 text-xl font-semibold text-primary">{strings.billingReports}</h2>
            <p className="mt-1 max-w-4xl text-sm leading-relaxed text-secondary">
              {strings.billingReportBasis(readable(period.from), readable(period.to))}
            </p>
          </div>
        </div>
        <Button
          variant="secondary"
          icon={<Download className="size-4" aria-hidden="true" />}
          onClick={() => void download()}
          disabled={report === null || downloading}
        >
          {strings.billingReportDownloadCsv}
        </Button>
      </header>

      <form
        className="mx-auto mb-6 grid w-full max-w-[90rem] shrink-0 grid-cols-[minmax(12rem,1fr)_minmax(12rem,1fr)_auto] items-end gap-4 rounded-xl border border-subtle bg-surface p-5 shadow-sm max-lg:grid-cols-2 max-sm:grid-cols-1"
        onSubmit={(e) => {
          e.preventDefault();
          setPeriod(form);
        }}
      >
        <label className="flex min-w-0 flex-col gap-2 text-xs font-semibold uppercase tracking-wide text-tertiary">
          <span>{strings.billingReportFrom}</span>
          <input
            className={dateInput}
            type="date"
            value={form.from}
            onChange={(e) => setForm({ ...form, from: e.target.value })}
            required
          />
        </label>
        <label className="flex min-w-0 flex-col gap-2 text-xs font-semibold uppercase tracking-wide text-tertiary">
          <span>{strings.billingReportTo}</span>
          <input
            className={dateInput}
            type="date"
            value={form.to}
            onChange={(e) => setForm({ ...form, to: e.target.value })}
            required
          />
        </label>
        <div className="flex items-center gap-2 max-lg:col-span-2 max-sm:col-span-1 max-sm:flex-wrap">
          <Button type="submit">{strings.billingReportShow}</Button>
          <button
            type="button"
            className={quickAction}
            onClick={() => pick(quarterOf(new Date()))}
          >
            {strings.billingReportThisQuarter}
          </button>
          <button
            type="button"
            className={quickAction}
            onClick={() => pick(previousQuarterOf(new Date()))}
          >
            {strings.billingReportLastQuarter}
          </button>
          {(loading || downloading) && <Spinner size={16} />}
        </div>
      </form>

      {error !== null && <ErrorBanner message={error} />}

      {loading ? (
        <div className="flex min-h-72 flex-1 items-center justify-center rounded-xl border border-subtle bg-surface" role="status" aria-label={strings.billingLoading}>
          <Spinner size={24} />
        </div>
      ) : report !== null && report.currencies.length === 0 ? (
        <section className="flex min-h-72 flex-1 flex-col items-center justify-center rounded-xl border border-subtle bg-surface px-6 py-12 text-center shadow-sm">
          <span className="mb-4 flex size-14 items-center justify-center rounded-2xl bg-[var(--accent-soft)] text-accent" aria-hidden="true">
            <CalendarRange className="size-6" />
          </span>
          <h2 className="m-0 text-lg font-semibold text-primary">{strings.billingReportEmptyTitle}</h2>
          <p className="mb-0 mt-2 max-w-lg text-sm leading-relaxed text-secondary">{strings.billingReportEmptyBody}</p>
        </section>
      ) : (
        report?.currencies.map((group) => (
          <section key={group.currency} className={`${tableShell} mx-auto w-full max-w-[90rem]`}>
            <div className="flex items-center justify-between border-b border-subtle px-5 py-4">
              <div>
                <h3 className="m-0 text-sm font-semibold text-primary">{strings.billingReportCaption(group.currency)}</h3>
                <p className="mb-0 mt-1 text-xs text-tertiary">
                  {strings.billingReportCounts(group.invoiceCount, group.creditNoteCount)}
                </p>
              </div>
              <span className="rounded-full bg-[var(--accent-soft)] px-3 py-1 text-xs font-semibold text-accent">
                {group.currency}
              </span>
            </div>
            <table className={tableClass}>
              <caption className="sr-only">
                {strings.billingReportCaption(group.currency)}
              </caption>
              <thead>
                <tr>
                  <th scope="col" className={headCell}>{strings.billingColVatRate}</th>
                  <th scope="col" className={`${headCell} ${numberCell}`}>
                    {strings.billingColNet}
                  </th>
                  <th scope="col" className={`${headCell} ${numberCell}`}>
                    {strings.billingReportColVat}
                  </th>
                </tr>
              </thead>
              <tbody>
                {group.byRate.map((rate) => (
                  <tr key={rate.rateBp}>
                    <td className={bodyCell}>{formatRate(rate.rateBp, locale)}</td>
                    <td className={`${bodyCell} ${numberCell}`}>
                      {formatAmount(rate.netCents, locale, group.currency)}
                    </td>
                    <td className={`${bodyCell} ${numberCell}`}>
                      {formatAmount(rate.vatCents, locale, group.currency)}
                    </td>
                  </tr>
                ))}
              </tbody>
              <tfoot className="bg-raised text-primary">
                <tr>
                  <th scope="row" className="border-t border-default px-4 py-3 text-left font-semibold">{strings.billingReportTotal}</th>
                  <td className={`border-t border-default px-4 py-3 ${numberCell}`}>
                    {formatAmount(group.netCents, locale, group.currency)}
                  </td>
                  <td className={`border-t border-default px-4 py-3 ${numberCell}`}>
                    {formatAmount(group.vatCents, locale, group.currency)}
                  </td>
                </tr>
                <tr>
                  <th scope="row" className="border-t border-default px-4 py-3 text-left font-semibold">{strings.billingReportGross}</th>
                  <td className={`border-t border-default px-4 py-3 ${numberCell}`} colSpan={2}>
                    {formatAmount(group.grossCents, locale, group.currency)}
                  </td>
                </tr>
              </tfoot>
            </table>
          </section>
        ))
      )}

      {/* The period in the accounting currency. Rendered only when the report
          holds something *and* it says something the tables above do not — a
          tenant that bills solely in its own currency would otherwise read the
          same figures twice. */}
      {report !== null && report.currencies.length > 0 && restatesAnything(report) && (
        <section className={tableShell}>
          <p className="m-0 border-b border-subtle px-4 py-3 text-sm text-secondary">
            {strings.billingReportBaseIntro(report.base.currency)}
          </p>
          {report.base.unconvertedCount > 0 && (
            <ErrorBanner message={strings.billingReportUnconverted(report.base.unconvertedCount)} />
          )}
          <table className={tableClass}>
            <caption className="sr-only">
              {strings.billingReportBaseCaption(report.base.currency)}
            </caption>
            <thead>
              <tr>
                <th scope="col" className={headCell}>{strings.billingColVatRate}</th>
                <th scope="col" className={`${headCell} ${numberCell}`}>
                  {strings.billingColNet}
                </th>
                <th scope="col" className={`${headCell} ${numberCell}`}>
                  {strings.billingReportColVat}
                </th>
              </tr>
            </thead>
            <tbody>
              {report.base.byRate.map((rate) => (
                <tr key={rate.rateBp}>
                  <td className={bodyCell}>{formatRate(rate.rateBp, locale)}</td>
                  <td className={`${bodyCell} ${numberCell}`}>
                    {formatAmount(rate.netCents, locale, report.base.currency)}
                  </td>
                  <td className={`${bodyCell} ${numberCell}`}>
                    {formatAmount(rate.vatCents, locale, report.base.currency)}
                  </td>
                </tr>
              ))}
            </tbody>
            <tfoot className="bg-raised text-primary">
              <tr>
                <th scope="row" className="border-t border-default px-4 py-3 text-left font-semibold">{strings.billingReportTotal}</th>
                <td className={`border-t border-default px-4 py-3 ${numberCell}`}>
                  {formatAmount(report.base.netCents, locale, report.base.currency)}
                </td>
                <td className={`border-t border-default px-4 py-3 ${numberCell}`}>
                  {formatAmount(report.base.vatCents, locale, report.base.currency)}
                </td>
              </tr>
              <tr>
                <th scope="row" className="border-t border-default px-4 py-3 text-left font-semibold">{strings.billingReportGross}</th>
                <td className={`border-t border-default px-4 py-3 ${numberCell}`} colSpan={2}>
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
