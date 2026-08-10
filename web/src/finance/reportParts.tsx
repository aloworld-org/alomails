// What the four report screens share: the dates they are asked for, and the
// button that saves what is on screen as a file.
//
// One toolbar rather than four is not only about repetition. The server refuses
// a report with no period on purpose — a figure printed under a heading nobody
// chose is the one thing a document copied into a year-end must not be — so the
// period is *applied on submit*, never on each keystroke: a half-typed date
// never becomes a request, and the figures on screen always belong to the days
// written above them. Four copies of that rule would eventually be three.
//
// The CSV is fetched with the session's token and saved from memory, never
// linked: the route is authenticated, and a plain `<a href>` downloads a `401`
// named like a report (`platform/download.ts`).
import { useCallback, useState } from "react";
import type { ReactNode } from "react";

import type { Period } from "../billing";
import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { saveTextFile } from "../platform/download";
import { financeMessage } from "./api";
import { dayLabel } from "./format";
import styles from "./FinanceModule.module.css";

/** One quick pick: what it is called, and the period it applies. */
export interface Pick {
  label: string;
  period: Period;
}

/** The toolbar of a report asked over two days. */
export function PeriodToolbar({
  form,
  picks,
  busy,
  canDownload,
  onForm,
  onApply,
  onDownload,
  children,
}: {
  form: Period;
  picks: Pick[];
  busy: boolean;
  canDownload: boolean;
  onForm: (period: Period) => void;
  onApply: (period: Period) => void;
  onDownload: () => void;
  children?: ReactNode;
}) {
  return (
    <form
      className={styles.toolbar}
      onSubmit={(e) => {
        e.preventDefault();
        onApply(form);
      }}
    >
      <label className={styles.periodField}>
        {strings.financeReportFrom}
        <input
          className={styles.periodInput}
          type="date"
          value={form.from}
          onChange={(e) => onForm({ ...form, from: e.target.value })}
          required
        />
      </label>
      <label className={styles.periodField}>
        {strings.financeReportTo}
        <input
          className={styles.periodInput}
          type="date"
          value={form.to}
          onChange={(e) => onForm({ ...form, to: e.target.value })}
          required
        />
      </label>
      <Button type="submit" variant="ghost">
        {strings.financeReportShow}
      </Button>
      {picks.map((pick) => (
        <button
          key={pick.label}
          type="button"
          className={styles.linkAction}
          onClick={() => {
            onForm(pick.period);
            onApply(pick.period);
          }}
        >
          {pick.label}
        </button>
      ))}
      {children}
      <span className={styles.toolbarSpacer} />
      {busy && <Spinner size={16} />}
      <Button variant="ghost" onClick={onDownload} disabled={!canDownload || busy}>
        {strings.financeReportDownloadCsv}
      </Button>
    </form>
  );
}

/** The toolbar of a report that stands on one day — a balance sheet, an
 *  ageing. Cumulative by definition: everything on or before the day counts,
 *  back to the day the books opened, so there is one date and not two. */
export function DayToolbar({
  form,
  picks,
  busy,
  canDownload,
  onForm,
  onApply,
  onDownload,
  children,
}: {
  form: string;
  picks: { label: string; on: string }[];
  busy: boolean;
  canDownload: boolean;
  onForm: (on: string) => void;
  onApply: (on: string) => void;
  onDownload: () => void;
  children?: ReactNode;
}) {
  return (
    <form
      className={styles.toolbar}
      onSubmit={(e) => {
        e.preventDefault();
        onApply(form);
      }}
    >
      <label className={styles.periodField}>
        {strings.financeReportOn}
        <input
          className={styles.periodInput}
          type="date"
          value={form}
          onChange={(e) => onForm(e.target.value)}
          required
        />
      </label>
      <Button type="submit" variant="ghost">
        {strings.financeReportShow}
      </Button>
      {picks.map((pick) => (
        <button
          key={pick.label}
          type="button"
          className={styles.linkAction}
          onClick={() => {
            onForm(pick.on);
            onApply(pick.on);
          }}
        >
          {pick.label}
        </button>
      ))}
      {children}
      <span className={styles.toolbarSpacer} />
      {busy && <Spinner size={16} />}
      <Button variant="ghost" onClick={onDownload} disabled={!canDownload || busy}>
        {strings.financeReportDownloadCsv}
      </Button>
    </form>
  );
}

/** The sentence above every report: which days these figures are the figures
 *  for. Repeated on each screen on purpose — a table with no period above it is
 *  a table somebody will read as "now". */
export function ReportBasis({ from, to }: { from: string; to?: string }) {
  return (
    <p className={styles.sectionNote}>
      {to === undefined
        ? strings.financeReportBasisOn(dayLabel(from, from))
        : strings.financeReportBasis(dayLabel(from, from), dayLabel(to, to))}
    </p>
  );
}

/**
 * What a report's downloading looks like from a screen: the state, the failure
 * and the one function a button calls.
 *
 * The file is named here rather than by the server's `Content-Disposition`,
 * because the bytes are already in memory by the time anything is saved; the
 * two names are kept identical on purpose (`finance_report_*.rs`), so a person
 * who saves the same report twice — once from the app, once from a script —
 * finds one file and not two.
 */
export function useCsvDownload(): {
  downloading: boolean;
  error: string | null;
  download: (fetchCsv: () => Promise<string>, fileName: string) => void;
} {
  const [downloading, setDownloading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const download = useCallback((fetchCsv: () => Promise<string>, fileName: string) => {
    setDownloading(true);
    void (async () => {
      try {
        saveTextFile(await fetchCsv(), fileName, "text/csv;charset=utf-8");
        setError(null);
      } catch (err) {
        // The server's own sentence when it sent one: a `422` here names the
        // date it could not read, which is a thing the person can fix.
        setError(financeMessage(err, strings.financeReportDownloadFailed));
      } finally {
        setDownloading(false);
      }
    })();
  }, []);
  return { downloading, error, download };
}
