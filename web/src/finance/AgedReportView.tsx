// Aged receivables and payables: who owes us, or whom we owe, by how overdue it
// is on one day.
//
// **The side is a choice, never a default.** What we are owed and what we owe
// are chased by different people and are read on different days; a screen that
// opened on one of them silently would eventually have somebody chasing the
// wrong list. Both are one control away, and the heading says which is on
// screen.
//
// **The bands are the server's, in the server's order.** `current`, 1–30, 31–60,
// 61–90, over 90 — spelled once in `alo_store::AgedBucket` — so the screen, the
// file and the wire cannot each choose their own.
//
// **A document that could not be restated is counted out loud.** Only amounts in
// the accounting currency are in the bands; a document whose currency has no
// rate frozen on it is in none of them, and printing the totals without saying
// so would be a listing that is quietly missing money.
import { useCallback, useEffect, useState } from "react";
import { Hourglass } from "lucide-react";

import { Select, Table, Td, Th } from "../ds";
import { strings } from "../i18n";
import { financeMessage, useFinanceApi } from "./api";
import { amountLabel, dayLabel, today } from "./format";
import { EmptyState, ErrorBanner } from "./parts";
import { DayToolbar, ReportBasis, useCsvDownload } from "./reportParts";
import type { AgedBuckets, AgedReport, AgedSide } from "./types";
import styles from "./FinanceModule.module.css";

export function AgedReportView() {
  const api = useFinanceApi();
  const [on, setOn] = useState<string>(() => today());
  const [form, setForm] = useState<string>(on);
  const [side, setSide] = useState<AgedSide>("receivable");
  const [report, setReport] = useState<AgedReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const csv = useCsvDownload();

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setReport(await api.agedReport(on, side));
      setError(null);
    } catch (err) {
      setError(financeMessage(err, strings.financeReportLoadFailed));
      setReport(null);
    } finally {
      setLoading(false);
    }
  }, [api, on, side]);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <div className={styles.page}>
      <DayToolbar
        form={form}
        picks={[{ label: strings.financeReportToday, on: today() }]}
        busy={loading || csv.downloading}
        canDownload={report !== null}
        onForm={setForm}
        onApply={setOn}
        onDownload={() =>
          csv.download(
            () => api.agedReportCsv(on, side),
            `aged-${side}-${on}.csv`,
          )
        }
      >
        {/* Which side, said in the toolbar rather than assumed. */}
        <label className={styles.periodField}>
          {strings.financeReportSide}
          <Select
            value={side}
            onChange={(e) => setSide(e.target.value as AgedSide)}
          >
            <option value="receivable">
              {strings.financeReportReceivable}
            </option>
            <option value="payable">{strings.financeReportPayable}</option>
          </Select>
        </label>
      </DayToolbar>

      <ReportBasis from={on} />
      {error !== null && <ErrorBanner message={error} />}
      {csv.error !== null && <ErrorBanner message={csv.error} />}

      {/* Money that is in no band, counted rather than quietly dropped. */}
      {report !== null && report.unconvertedCount > 0 && (
        <ErrorBanner
          message={strings.financeReportUnconverted(report.unconvertedCount)}
        />
      )}

      {report !== null && report.parties.length === 0 && !loading ? (
        <EmptyState
          Icon={Hourglass}
          title={
            side === "receivable"
              ? strings.financeReportNothingOwedToUs
              : strings.financeReportNothingWeOwe
          }
          body={strings.financeReportAgedEmptyBody}
        />
      ) : (
        report !== null && (
          <Table
            label={
              side === "receivable"
                ? strings.financeReportReceivable
                : strings.financeReportPayable
            }
          >
            <thead>
              <tr>
                <Th>{strings.financeReportParty}</Th>
                <Th numeric>{strings.financeReportBandCurrent}</Th>
                <Th numeric>{strings.financeReportBand1To30}</Th>
                <Th numeric>{strings.financeReportBand31To60}</Th>
                <Th numeric>{strings.financeReportBand61To90}</Th>
                <Th numeric>{strings.financeReportBand90Plus}</Th>
                <Th numeric>{strings.financeReportTotal}</Th>
              </tr>
            </thead>
            <tbody>
              {report.parties.map((party) => (
                <tr key={party.partyId}>
                  <Td>
                    {party.name}
                    {/* What is behind the bands: the oldest document is the
                        one somebody is about to be asked about. */}
                    <span className={styles.subtle}>
                      {strings.financeReportOpenDocuments(
                        party.documents.length,
                      )}
                      {party.documents.length > 0 &&
                        ` · ${party.documents[0]?.number ?? ""} ${dayLabel(
                          party.documents[0]?.dueDate ?? null,
                          "",
                        )}`}
                    </span>
                  </Td>
                  <Bands buckets={party.buckets} currency={report.currency} />
                </tr>
              ))}
            </tbody>
            <tfoot>
              <tr>
                <Th scope="row">{strings.financeReportTotal}</Th>
                <Bands buckets={report.buckets} currency={report.currency} />
              </tr>
            </tfoot>
          </Table>
        )
      )}
    </div>
  );
}

/** The five bands and their total, as the six numeric cells of a row. */
function Bands({
  buckets,
  currency,
}: {
  buckets: AgedBuckets;
  currency: string;
}) {
  return (
    <>
      {[
        buckets.currentCents,
        buckets.d1_30Cents,
        buckets.d31_60Cents,
        buckets.d61_90Cents,
        buckets.d90_plusCents,
        buckets.totalCents,
      ].map((cents, index) => (
        <Td key={index} numeric>
          {amountLabel(cents, currency)}
        </Td>
      ))}
    </>
  );
}
