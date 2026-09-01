// Value by stage and win/loss for one board (alo CRM, B2.08) — the third tab,
// and the only screen in CRM that shows a total.
//
// Three things it deliberately does not do. It does not compute money: every
// figure is the server's integer cents, and there is no column here the browser
// adds up. It does not add currencies together — the API answers one group per
// currency and refuses to convert a forecast, so this renders one table each.
// And it does not decide what the period contains.
//
// What it does say out loud is that the two halves of the report are answered
// differently: the stage rows are the **open board as it stands now**, while
// won and lost are the deals that closed between the two days. A screen that
// put both under one date range would be quietly wrong about one of them.
//
// The period is applied on submit rather than on every keystroke, so a
// half-typed date never becomes a request.
import { useCallback, useEffect, useState } from "react";
import { BarChart3, CalendarRange, Download, Trophy, TrendingDown, TrendingUp } from "lucide-react";
import type { LucideIcon } from "lucide-react";

import {
  formatAmount,
  formatRate,
  quarterOf,
  type Period,
} from "../billing";
import { Button, Spinner, Table, Td, Th } from "../ds";
import { strings, useLocale } from "../i18n";
import { saveTextFile } from "../platform/download";
import { crmMessage, useCrmApi } from "./api";
import { dayLabel, momentLabel } from "./format";
import { EmptyState, ErrorBanner } from "./parts";
import { ReportPeriodPicker } from "./ReportPeriodPicker";
import type { PipelineCurrency, PipelineReport } from "./types";

interface Props {
  /** The board the report is about; `null` while none has been chosen. */
  pipelineId: string | null;
  /** Bumped when a deal changed, so the figures follow the board. */
  revision: number;
}

/** What a saved report is called: the server names the file in its own
 *  `Content-Disposition`, and this is the same name for the copy the browser
 *  writes from memory (the route is authenticated, so the file is fetched
 *  rather than linked). */
function fileName(pipelineId: string, period: Period): string {
  return `pipeline-${pipelineId}-${period.from}-to-${period.to}.csv`;
}

export function ReportView({ pipelineId, revision }: Props) {
  const api = useCrmApi();
  const locale = useLocale();
  // The form opens on the quarter being lived through; the one being reviewed
  // is one click away.
  const [period, setPeriod] = useState<Period>(() => quarterOf(new Date()));
  const [report, setReport] = useState<PipelineReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [downloading, setDownloading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    if (pipelineId === null) return;
    setLoading(true);
    try {
      setReport(await api.pipelineReport(pipelineId, period.from, period.to));
      setError(null);
    } catch (err) {
      // The server's own sentence when it sent one — it names the rule that was
      // broken ("the period ends before it starts").
      setError(crmMessage(err, strings.crmLoadFailed));
      setReport(null);
    } finally {
      setLoading(false);
    }
  }, [api, pipelineId, period, revision]);

  useEffect(() => {
    void load();
  }, [load]);

  async function download() {
    if (pipelineId === null) return;
    setDownloading(true);
    try {
      const csv = await api.pipelineReportCsv(
        pipelineId,
        period.from,
        period.to,
      );
      saveTextFile(csv, fileName(pipelineId, period), "text/csv;charset=utf-8");
      setError(null);
    } catch (err) {
      setError(crmMessage(err, strings.crmReportDownloadFailed));
    } finally {
      setDownloading(false);
    }
  }

  return (
    <div className="mx-auto flex w-full max-w-[112rem] flex-col gap-5 overflow-auto px-8 py-6 max-[52rem]:px-4 max-[52rem]:py-4">
      <section className="rounded-xl border border-subtle bg-surface p-5 shadow-sm" aria-label={strings.crmReportPeriod}>
        <div className="flex flex-wrap items-center gap-3">
          <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent" aria-hidden="true">
            <CalendarRange size={19} />
          </span>
          <div className="min-w-0 flex-1">
            <h2 className="m-0 text-base font-semibold text-primary">{strings.crmReportPeriod}</h2>
            {report !== null && (
              <p className="mb-0 mt-1 text-xs text-secondary">
                {strings.crmReportBasis(dayLabel(report.from), dayLabel(report.to))} {strings.crmReportOpenAsOf(momentLabel(report.openAsOf))}
              </p>
            )}
          </div>
          {(loading || downloading) && <Spinner size={16} />}
          <Button variant="ghost" size="sm" icon={<Download />} onClick={() => void download()} disabled={report === null || downloading}>
            {strings.crmReportDownloadCsv}
          </Button>
        </div>

        <div className="mt-5">
          <ReportPeriodPicker value={period} onApply={setPeriod} />
        </div>
      </section>

      {error !== null && <ErrorBanner message={error} />}

      {report !== null && report.currencies.length === 0 && !loading ? (
        <EmptyState
          Icon={BarChart3}
          title={strings.crmReportEmptyTitle}
          body={strings.crmReportEmptyBody}
        />
      ) : (
        report?.currencies.map((group) => (
          <CurrencyTables key={group.currency} group={group} locale={locale} />
        ))
      )}
    </div>
  );
}

/** One currency: the open board column by column, then what closed in the
 *  period. Two tables rather than one, because they answer two questions and a
 *  single table would invite reading a column total across both. */
function CurrencyTables({
  group,
  locale,
}: {
  group: PipelineCurrency;
  locale: string;
}) {
  const money = (cents: number) => formatAmount(cents, locale, group.currency);
  return (
    // Two tables and a sentence, in their own column. Each table is its own
    // labelled, scrollable region now — the caption stays drawn, because the
    // two answer different questions and have to say which.
    <section className="rounded-xl border border-subtle bg-surface p-5 shadow-sm">
      <div className="flex flex-wrap items-center gap-3">
        <div className="min-w-0 flex-1">
          <p className="m-0 text-xs font-semibold uppercase tracking-wide text-accent">{group.currency}</p>
          <h2 className="mb-0 mt-1 text-lg font-semibold text-primary">{strings.crmReportOpenCaption(group.currency)}</h2>
        </div>
      </div>

      <div className="mt-5 grid grid-cols-4 gap-3 max-xl:grid-cols-2 max-sm:grid-cols-1">
        <ReportMetric Icon={BarChart3} label={strings.crmReportOpenTotal} value={money(group.open.valueCents)} detail={`${group.open.dealCount} ${strings.crmReportColDeals}`} />
        <ReportMetric Icon={TrendingUp} tone="success" label={strings.crmStateWon} value={money(group.won.valueCents)} detail={`${group.won.dealCount} ${strings.crmReportColDeals}`} />
        <ReportMetric Icon={TrendingDown} tone="danger" label={strings.crmStateLost} value={money(group.lost.valueCents)} detail={`${group.lost.dealCount} ${strings.crmReportColDeals}`} />
        <ReportMetric Icon={Trophy} label={strings.crmReportWinRateLabel} value={group.winRateBp === null ? "—" : formatRate(group.winRateBp, locale)} detail={group.winRateBp === null ? `0 ${strings.crmReportClosedDeals}` : `${group.won.dealCount} ${strings.crmStateWon} · ${group.won.dealCount + group.lost.dealCount} ${strings.crmReportClosedDeals}`} />
      </div>

      <div className="mt-5 grid grid-cols-2 gap-4 max-lg:grid-cols-1">
        <article className="overflow-hidden rounded-xl border border-subtle bg-surface">
          <h3 className="m-0 border-b border-subtle bg-raised/35 px-4 py-3 text-sm font-semibold text-primary">{strings.crmReportOpenCaption(group.currency)}</h3>
          <Table label={strings.crmReportOpenCaption(group.currency)} density="compact" flat>
        <thead>
          <tr>
            <Th>{strings.crmColStage}</Th>
            <Th numeric>{strings.crmReportColDeals}</Th>
            <Th numeric>{strings.crmColValue}</Th>
          </tr>
        </thead>
        <tbody>
          {group.stages.map((row) => (
            <tr key={row.stageId}>
              <Td>{row.name}</Td>
              <Td numeric>{row.open.dealCount}</Td>
              <Td numeric>{money(row.open.valueCents)}</Td>
            </tr>
          ))}
        </tbody>
        <tfoot>
          <tr>
            <Th scope="row">{strings.crmReportOpenTotal}</Th>
            <Td numeric>{group.open.dealCount}</Td>
            <Td numeric>{money(group.open.valueCents)}</Td>
          </tr>
        </tfoot>
          </Table>
        </article>

        <article className="overflow-hidden rounded-xl border border-subtle bg-surface">
          <h3 className="m-0 border-b border-subtle bg-raised/35 px-4 py-3 text-sm font-semibold text-primary">{strings.crmReportClosedCaption(group.currency)}</h3>
          <Table label={strings.crmReportClosedCaption(group.currency)} density="compact" flat>
        <thead>
          <tr>
            <Th>{strings.crmColState}</Th>
            <Th numeric>{strings.crmReportColDeals}</Th>
            <Th numeric>{strings.crmColValue}</Th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <Td>{strings.crmStateWon}</Td>
            <Td numeric>{group.won.dealCount}</Td>
            <Td numeric>{money(group.won.valueCents)}</Td>
          </tr>
          <tr>
            <Td>{strings.crmStateLost}</Td>
            <Td numeric>{group.lost.dealCount}</Td>
            <Td numeric>{money(group.lost.valueCents)}</Td>
          </tr>
        </tbody>
          </Table>
        </article>
      </div>
    </section>
  );
}

function ReportMetric({ Icon, label, value, detail, tone = "accent" }: { Icon: LucideIcon; label: string; value: string; detail: string; tone?: "accent" | "success" | "danger" }) {
  const color = tone === "success" ? "bg-success/10 text-success" : tone === "danger" ? "bg-danger/10 text-danger" : "bg-accent-soft text-accent";
  return (
    <article aria-label={label} className="flex min-w-0 items-center gap-3 rounded-xl border border-subtle bg-raised/25 p-4">
      <span className={`grid size-10 shrink-0 place-items-center rounded-xl ${color}`} aria-hidden="true"><Icon size={18} /></span>
      <div className="min-w-0">
        <p className="m-0 text-xs font-medium text-secondary">{label}</p>
        <p className="mb-0 mt-1 truncate text-lg font-semibold tabular-nums text-primary">{value}</p>
        <p className="mb-0 mt-0.5 text-xs text-tertiary">{detail}</p>
      </div>
    </article>
  );
}
