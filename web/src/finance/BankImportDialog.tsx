// Importing a statement: pick the file, read what the server made of it, fix
// the reading if it is wrong, and only then commit.
//
// **Two steps, and the first one writes nothing.** `POST
// /finance/imports/bank/preview` is a pure reading of the bytes, so the person
// who is about to stage three hundred transactions sees the server's own
// understanding of them first — which columns it took for the date and the
// amount, which convention it read the numbers in, how many rows it could not
// read at all. Nothing here parses the file: a browser that read the CSV itself
// would be a second reader, and the two would disagree on exactly the files
// that matter (a Windows-1252 export with comma decimals and a `Soll/Haben`
// column).
//
// **The mapping is only ever the file's own header.** Every mapping control is a
// `<select>` over the columns the server reported, never a text box: a column
// name typed by hand is a `422` waiting to happen, and the file already says
// what its columns are called. They appear only for a CSV — a CAMT.053 or an
// MT940 states its own dates, currency and account, and offering to override
// them would invent a question the format has already answered.
//
// **A refusal is shown in full.** A file with one unreadable row imports
// nothing and comes back as a `422` carrying the same report a preview would
// have shown; this dialog renders that report rather than the sentence alone,
// because a refusal a person cannot act on is the one thing an importer must
// never answer.
import { useState } from "react";
import { Landmark } from "lucide-react";

import { Button, Field, Input, Select, Table, Td, Th } from "../ds";
import { strings } from "../i18n";
import { BankImportRefused, financeMessage, useFinanceApi } from "./api";
import { amountLabel, dayLabel, sourceLabel } from "./format";
import { DialogFrame, ErrorBanner } from "./parts";
import type {
  BankCsvMapping,
  BankImportOptions,
  BankImportReport,
} from "./types";
import styles from "./FinanceModule.module.css";

/** The mapping fields, in the order a person reads a bank export left to right:
 *  when, how much, and who. The label is the question the column answers. */
const MAPPING_FIELDS: { key: keyof BankCsvMapping; label: () => string }[] = [
  { key: "date", label: () => strings.financeBankColDate },
  { key: "valueDate", label: () => strings.financeBankColValueDate },
  { key: "amount", label: () => strings.financeBankColAmount },
  { key: "debit", label: () => strings.financeBankColDebit },
  { key: "credit", label: () => strings.financeBankColCredit },
  { key: "sign", label: () => strings.financeBankColSign },
  { key: "currencyColumn", label: () => strings.financeBankColCurrency },
  { key: "counterparty", label: () => strings.financeBankColCounterparty },
  { key: "iban", label: () => strings.financeBankColIban },
  { key: "remittance", label: () => strings.financeBankColRemittance },
  { key: "reference", label: () => strings.financeBankColReference },
];

/** How the file writes its days, as the server names the conventions. */
const DATE_CONVENTIONS = ["auto", "dmy", "mdy", "ymd"] as const;
/** What separates the cents. */
const DECIMAL_CONVENTIONS = ["auto", "comma", "dot"] as const;

/** An empty mapping — every column unmapped, which is the server's "guess". */
const NO_MAPPING: BankCsvMapping = {
  date: null,
  valueDate: null,
  amount: null,
  debit: null,
  credit: null,
  sign: null,
  currencyColumn: null,
  counterparty: null,
  iban: null,
  remittance: null,
  reference: null,
};

export function BankImportDialog({
  onClose,
  onImported,
}: {
  onClose: () => void;
  /** The commit landed. The Bank tab reloads its statements on this. */
  onImported: (report: BankImportReport) => void;
}) {
  const api = useFinanceApi();
  const [file, setFile] = useState<File | null>(null);
  const [account, setAccount] = useState("");
  const [currency, setCurrency] = useState("");
  const [dates, setDates] = useState<string>("auto");
  const [decimal, setDecimal] = useState<string>("auto");
  const [mapping, setMapping] = useState<BankCsvMapping>(NO_MAPPING);
  const [report, setReport] = useState<BankImportReport | null>(null);
  // The reading on screen is no longer the reading the form describes. Set by
  // every mapping control, cleared by the next preview.
  const [stale, setStale] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  /** Everything the upload states about the file. The mapping travels on a
   *  preview *and* on the commit: the two calls must read the same file the
   *  same way, and the server holds nothing between them. */
  function options(): BankImportOptions {
    return { account, currency, dates, decimal, mapping };
  }

  /** A new file is a new reading: keeping the old report beside it would show a
   *  sample of a file the person is no longer importing. */
  function chooseFile(chosen: File | null) {
    setFile(chosen);
    setReport(null);
    setStale(false);
    setError(null);
  }

  /** Correcting the reading makes the report on screen stale, and the primary
   *  action goes back to "check this file". What a person is shown and what
   *  they commit are then always the same reading — an Import button that
   *  staged a mapping nobody previewed would make the dry run advisory.
   *
   *  The stale report stays visible on purpose: it is the sample the person is
   *  correcting *against*, and blanking the screen on every keystroke would
   *  take away the thing they are reading. */
  function correct(change: () => void) {
    change();
    setStale(true);
  }

  /** What the file **would** stage. Writes nothing, which is why it can be run
   *  as often as it takes to get the mapping right. */
  async function preview() {
    if (file === null) return;
    setBusy(true);
    setError(null);
    try {
      const read = await api.previewBankImport(file, options());
      setReport(read);
      setStale(false);
      // The server's own guess becomes the form's state, so the selects show
      // what it actually used rather than an empty box beside a correct guess.
      setMapping(read.mapping);
      setDates(read.dates);
      setDecimal(read.decimal);
    } catch (err) {
      setReport(null);
      setStale(false);
      setError(financeMessage(err, strings.financeBankReadFailed));
    } finally {
      setBusy(false);
    }
  }

  /** Stages the file. A `422` carries the report, and the report is what the
   *  person needs — so it replaces the preview rather than being reduced to the
   *  sentence above it. */
  async function commit() {
    if (file === null || report === null || stale) return;
    setBusy(true);
    setError(null);
    try {
      const done = await api.importBankFile(file, options());
      onImported(done);
    } catch (err) {
      if (err instanceof BankImportRefused) {
        setReport(err.report);
        setStale(false);
      }
      setError(financeMessage(err, strings.financeBankImportFailed));
    } finally {
      setBusy(false);
    }
  }

  const csv = report !== null && report.source === "csv";
  const columns = report?.columns ?? [];
  const readable =
    report !== null && report.counts.lines > 0 && report.errors.length === 0;
  // Which act the primary button performs. Anything but a fresh, readable
  // report means the next step is to look again, never to stage.
  const checking = report === null || stale || !readable;

  return (
    <DialogFrame
      Icon={Landmark}
      title={strings.financeBankImportTitle}
      subtitle={strings.financeBankImportSubtitle}
      error={error}
      busy={busy}
      canSubmit={file !== null}
      submitLabel={
        checking ? strings.financeBankCheckFile : strings.financeBankImport
      }
      onClose={onClose}
      onSubmit={() => void (checking ? preview() : commit())}
    >
      <Field label={strings.financeBankFile} hint={strings.financeBankFileHint}>
        {(control) => (
          <Input
            {...control}
            type="file"
            onChange={(e) => chooseFile(e.target.files?.[0] ?? null)}
          />
        )}
      </Field>

      <div className={styles.row}>
        <Field
          label={strings.financeBankAccount}
          hint={strings.financeBankAccountHint}
        >
          {(control) => (
            <Input
              {...control}
              value={account}
              onChange={(e) => setAccount(e.target.value)}
              placeholder="DE02 1203 0000 0000 2020 51"
              autoComplete="off"
            />
          )}
        </Field>
        <Field
          label={strings.financeCurrency}
          hint={strings.financeBankCurrencyHint}
        >
          {(control) => (
            <Input
              {...control}
              value={currency}
              onChange={(e) => setCurrency(e.target.value.toUpperCase())}
              maxLength={3}
              autoComplete="off"
            />
          )}
        </Field>
      </div>

      {report !== null && (
        <>
          <ReadingSummary report={report} />

          {csv && (
            <section className={styles.section}>
              <h3 className={styles.sectionTitle}>
                {strings.financeBankMappingTitle}
              </h3>
              <p className={styles.sectionNote}>
                {strings.financeBankMappingNote}
              </p>
              <div className={styles.row}>
                <Field label={strings.financeBankDates}>
                  {(control) => (
                    <Select
                      {...control}
                      fullWidth
                      value={dates}
                      onChange={(e) => correct(() => setDates(e.target.value))}
                    >
                      {DATE_CONVENTIONS.map((convention) => (
                        <option key={convention} value={convention}>
                          {conventionLabel(convention)}
                        </option>
                      ))}
                    </Select>
                  )}
                </Field>
                <Field label={strings.financeBankDecimal}>
                  {(control) => (
                    <Select
                      {...control}
                      fullWidth
                      value={decimal}
                      onChange={(e) =>
                        correct(() => setDecimal(e.target.value))
                      }
                    >
                      {DECIMAL_CONVENTIONS.map((convention) => (
                        <option key={convention} value={convention}>
                          {conventionLabel(convention)}
                        </option>
                      ))}
                    </Select>
                  )}
                </Field>
              </div>
              <div className={styles.mappingGrid}>
                {MAPPING_FIELDS.map((field) => (
                  <Field key={field.key} label={field.label()}>
                    {(control) => (
                      // "This file has no such column" is an answer, not a
                      // prompt: most exports leave several of the eleven empty.
                      <Select
                        {...control}
                        fullWidth
                        placeholder={strings.financeBankColumnNone}
                        value={mapping[field.key] ?? ""}
                        onChange={(e) =>
                          correct(() =>
                            setMapping({
                              ...mapping,
                              [field.key]:
                                e.target.value === "" ? null : e.target.value,
                            }),
                          )
                        }
                      >
                        {columns.map((column) => (
                          <option key={column} value={column}>
                            {column}
                          </option>
                        ))}
                      </Select>
                    )}
                  </Field>
                ))}
              </div>
              {stale && (
                <p className={styles.sectionNote}>{strings.financeBankStale}</p>
              )}
              <Button
                variant="ghost"
                onClick={() => void preview()}
                disabled={busy}
              >
                {strings.financeBankCheckAgain}
              </Button>
            </section>
          )}

          {report.errors.length > 0 && (
            <section className={styles.section}>
              <ErrorBanner
                message={strings.financeBankRowsRefused(report.errors.length)}
              />
              <ul className={styles.rowErrors}>
                {report.errors.map((row, index) => (
                  <li key={`${row.line ?? "?"}-${index}`}>
                    <strong>
                      {row.line === null
                        ? strings.financeBankRowUnknown
                        : strings.financeBankRowAt(row.line)}
                    </strong>{" "}
                    {row.rule}
                  </li>
                ))}
              </ul>
            </section>
          )}

          <SamplePreview report={report} />
        </>
      )}
    </DialogFrame>
  );
}

/** What the server made of the file, in one line per fact it decided. */
function ReadingSummary({ report }: { report: BankImportReport }) {
  return (
    <dl className={styles.summary}>
      <div>
        <dt>{strings.financeBankFormat}</dt>
        <dd>{sourceLabel(report.source)}</dd>
      </div>
      <div>
        <dt>{strings.financeBankRows}</dt>
        <dd>
          {strings.financeBankRowsRead(report.counts.lines, report.totalRows)}
        </dd>
      </div>
      {report.counts.skipped > 0 && (
        <div>
          <dt>{strings.financeBankSkipped}</dt>
          <dd>{report.counts.skipped}</dd>
        </div>
      )}
      {report.counts.unbooked !== null && report.counts.unbooked > 0 && (
        <div>
          <dt>{strings.financeBankUnbooked}</dt>
          <dd>{report.counts.unbooked}</dd>
        </div>
      )}
      {report.account !== null && (
        <div>
          <dt>{strings.financeBankAccount}</dt>
          <dd>{report.account}</dd>
        </div>
      )}
      {report.period !== null && (
        <div>
          <dt>{strings.financeBankPeriod}</dt>
          <dd>
            {dayLabel(report.period.from, "—")} –{" "}
            {dayLabel(report.period.to, "—")}
          </dd>
        </div>
      )}
      {report.encoding !== null && (
        <div>
          <dt>{strings.financeBankEncoding}</dt>
          <dd>
            {report.encoding}
            {report.delimiter !== null && ` · ${report.delimiter}`}
          </dd>
        </div>
      )}
    </dl>
  );
}

/** The first transactions, as the server read them — the point of the dry run:
 *  a person checks that the columns line up, not that the year is right. */
function SamplePreview({ report }: { report: BankImportReport }) {
  if (report.sample.length === 0) return null;
  return (
    <section className={styles.section}>
      <h3 className={styles.sectionTitle}>{strings.financeBankSampleTitle}</h3>
      <Table label={strings.financeBankSampleTable}>
        <thead>
          <tr>
            <Th>{strings.financeBankBookedOn}</Th>
            <Th>{strings.financeBankCounterparty}</Th>
            <Th>{strings.financeBankRemittance}</Th>
            <Th numeric>{strings.financeGross}</Th>
          </tr>
        </thead>
        <tbody>
          {report.sample.map((line, index) => (
            <tr key={`${line.line ?? index}`}>
              <Td>{dayLabel(line.bookedOn, "—")}</Td>
              <Td>{line.counterpartyName ?? ""}</Td>
              <Td className={styles.muted}>{line.remittance ?? ""}</Td>
              <Td numeric>{amountLabel(line.amountCents, line.currency)}</Td>
            </tr>
          ))}
        </tbody>
      </Table>
      {report.sampleTruncated && (
        <p className={styles.sectionNote}>
          {strings.financeBankSampleTruncated}
        </p>
      )}
    </section>
  );
}

/** A reading convention in words. Unknown ones are shown verbatim — the server
 *  may learn one before this client does. */
function conventionLabel(convention: string): string {
  switch (convention) {
    case "auto":
      return strings.financeBankConventionAuto;
    case "dmy":
      return strings.financeBankConventionDmy;
    case "mdy":
      return strings.financeBankConventionMdy;
    case "ymd":
      return strings.financeBankConventionYmd;
    case "comma":
      return strings.financeBankConventionComma;
    case "dot":
      return strings.financeBankConventionDot;
    default:
      return convention;
  }
}
