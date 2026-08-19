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
import { BarChart3 } from "lucide-react";

import {
  formatAmount,
  formatRate,
  previousQuarterOf,
  quarterOf,
  type Period,
} from "../billing";
import { Button, Field, Input, Spinner, Table, Td, Th, Toolbar } from "../ds";
import { strings, useLocale } from "../i18n";
import { saveTextFile } from "../platform/download";
import { crmMessage, useCrmApi } from "./api";
import { dayLabel, momentLabel } from "./format";
import { EmptyState, ErrorBanner } from "./parts";
import type { PipelineCurrency, PipelineReport } from "./types";
import styles from "./CrmModule.module.css";

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
  const [form, setForm] = useState<Period>(period);
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

  /** Applies a period from a quick pick: the form and the request move
   *  together, so what is shown always matches what is written. */
  function pick(next: Period) {
    setForm(next);
    setPeriod(next);
  }

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
    <div className={styles.page}>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          setPeriod(form);
        }}
      >
        {/* `align="end"` because this row is labelled fields beside buttons: on
            centres, the labels drag the two date fields out of line. */}
        <Toolbar label={strings.crmReportPeriod} align="end">
          <Field label={strings.crmReportFrom}>
            {(control) => (
              <Input
                {...control}
                type="date"
                value={form.from}
                onChange={(e) => setForm({ ...form, from: e.target.value })}
                required
              />
            )}
          </Field>
          <Field label={strings.crmReportTo}>
            {(control) => (
              <Input
                {...control}
                type="date"
                value={form.to}
                onChange={(e) => setForm({ ...form, to: e.target.value })}
                required
              />
            )}
          </Field>
          <Button type="submit">{strings.crmReportShow}</Button>
          <button
            type="button"
            className={styles.linkAction}
            onClick={() => pick(quarterOf(new Date()))}
          >
            {strings.crmReportThisQuarter}
          </button>
          <button
            type="button"
            className={styles.linkAction}
            onClick={() => pick(previousQuarterOf(new Date()))}
          >
            {strings.crmReportLastQuarter}
          </button>
          {(loading || downloading) && <Spinner size={16} />}
          <Button
            variant="ghost"
            onClick={() => void download()}
            disabled={report === null || downloading}
          >
            {strings.crmReportDownloadCsv}
          </Button>
        </Toolbar>
      </form>

      {error !== null && <ErrorBanner message={error} />}

      {report !== null && (
        <p className={styles.reportBasis}>
          {strings.crmReportBasis(dayLabel(report.from), dayLabel(report.to))}{" "}
          {strings.crmReportOpenAsOf(momentLabel(report.openAsOf))}
        </p>
      )}

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
    <section className="flex flex-col gap-3">
      <Table label={strings.crmReportOpenCaption(group.currency)} showLabel>
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

      <Table label={strings.crmReportClosedCaption(group.currency)} showLabel>
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
      {/* A win rate over no closed deals is unanswered, not zero — so the
          sentence is absent rather than reading "0 %". */}
      <p className={styles.reportBasis}>
        {group.winRateBp === null
          ? strings.crmReportNoWinRate
          : strings.crmReportWinRate(
              formatRate(group.winRateBp, locale),
              group.won.dealCount,
              group.won.dealCount + group.lost.dealCount,
            )}
      </p>
    </section>
  );
}
