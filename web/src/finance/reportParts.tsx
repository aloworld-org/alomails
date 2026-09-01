// What the four report screens share: the dates they are asked for, the button
// that saves what is on screen as a file, and the heading row that divides one
// report table into its sides.
//
// One toolbar rather than four is not only about repetition. The server refuses
// a report with no period on purpose — a figure printed under a heading nobody
// chose is the one thing a document copied into a year-end must not be — so the
// period is *applied on submit*, never on each keystroke: a half-typed date
// never becomes a request, and the figures on screen always belong to the days
// written above them. Four copies of that rule would eventually be three.
//
// The `<form>` is still the element that carries the submit; `ds/Toolbar` is
// the row inside it, because a toolbar is a named group of controls and a form
// is what Enter acts on. Wrapping rather than replacing keeps both facts true.
//
// The CSV is fetched with the session's token and saved from memory, never
// linked: the route is authenticated, and a plain `<a href>` downloads a `401`
// named like a report (`platform/download.ts`).
import { useCallback, useState } from "react";
import type { ReactNode } from "react";

import type { Period } from "../billing";
import { Button, DatePicker, Spinner, Th, Toolbar, ToolbarSpacer } from "../ds";
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
      onSubmit={(e) => {
        e.preventDefault();
        onApply(form);
      }}
    >
      <Toolbar label={strings.financeReportPeriod}>
        <label className={styles.periodField}>
          {strings.financeReportFrom}
          <DatePicker
            value={form.from}
            onChange={(from) => onForm({ ...form, from })}
          />
        </label>
        <label className={styles.periodField}>
          {strings.financeReportTo}
          <DatePicker
            value={form.to}
            onChange={(to) => onForm({ ...form, to })}
          />
        </label>
        <Button type="submit" variant="ghost">
          {strings.financeReportShow}
        </Button>
        {picks.map((pick) => (
          <Button
            key={pick.label}
            variant="ghost"
            size="sm"
            onClick={() => {
              onForm(pick.period);
              onApply(pick.period);
            }}
          >
            {pick.label}
          </Button>
        ))}
        {children}
        <ToolbarSpacer />
        {busy && <Spinner size={16} />}
        <Button
          variant="ghost"
          onClick={onDownload}
          disabled={!canDownload || busy}
        >
          {strings.financeReportDownloadCsv}
        </Button>
      </Toolbar>
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
      onSubmit={(e) => {
        e.preventDefault();
        onApply(form);
      }}
    >
      <Toolbar label={strings.financeReportPeriod}>
        <label className={styles.periodField}>
          {strings.financeReportOn}
          <DatePicker value={form} onChange={onForm} />
        </label>
        <Button type="submit" variant="ghost">
          {strings.financeReportShow}
        </Button>
        {picks.map((pick) => (
          <Button
            key={pick.label}
            variant="ghost"
            size="sm"
            onClick={() => {
              onForm(pick.on);
              onApply(pick.on);
            }}
          >
            {pick.label}
          </Button>
        ))}
        {children}
        <ToolbarSpacer />
        {busy && <Spinner size={16} />}
        <Button
          variant="ghost"
          onClick={onDownload}
          disabled={!canDownload || busy}
        >
          {strings.financeReportDownloadCsv}
        </Button>
      </Toolbar>
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

/** A heading row inside a report table — "Income", "What is owned".
 *
 *  A row of the table rather than a table of its own, so both sides of a result
 *  share one column width and read as one document.
 *
 *  Every utility is written through `[&[data-section]]:` on purpose. `ds/Table`
 *  styles its cells with descendant utilities (`[&_th]:text-tertiary`), which
 *  are one class *and* one element and therefore outrank a plain utility on the
 *  cell itself — the same specificity fact `Th`'s own `TH_ALIGN` had to answer,
 *  and the same move the stylesheet made when it wrote `.table .sectionTitle`.
 *  One class and one attribute beats both. */
const SECTION =
  "[&[data-section]]:pt-3 [&[data-section]]:text-xs [&[data-section]]:uppercase " +
  "[&[data-section]]:tracking-[0.04em] [&[data-section]]:text-secondary";

export function SectionHeading({
  title,
  cols,
}: {
  title: string;
  cols: number;
}) {
  return (
    <tr>
      <Th scope="colgroup" colSpan={cols} data-section="" className={SECTION}>
        {title}
      </Th>
    </tr>
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
  const download = useCallback(
    (fetchCsv: () => Promise<string>, fileName: string) => {
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
    },
    [],
  );
  return { downloading, error, download };
}
