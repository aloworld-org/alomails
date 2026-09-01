import { useCallback, useEffect, useState } from "react";
import { CalendarClock, FileSpreadsheet, Plus, Trash2 } from "lucide-react";

import {
  Badge,
  Button,
  Card,
  DatePicker,
  Input,
  Select,
  Spinner,
  useDialogs,
} from "../ds";
import { strings } from "../i18n";
import { financeMessage, useFinanceApi } from "./api";
import { ErrorBanner } from "./parts";
import type {
  FinanceReportCadence,
  FinanceReportKind,
  FinanceReportSchedule,
} from "./types";

const REPORTS: FinanceReportKind[] = [
  "pl",
  "balance",
  "aged_receivable",
  "aged_payable",
  "vat",
];
const CADENCES: FinanceReportCadence[] = ["weekly", "monthly", "quarterly"];
const tomorrow = () =>
  new Date(Date.now() + 86_400_000).toISOString().slice(0, 10);

export function ReportSchedulesView() {
  const api = useFinanceApi();
  const dialogs = useDialogs();
  const [rows, setRows] = useState<FinanceReportSchedule[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [report, setReport] = useState<FinanceReportKind>("pl");
  const [cadence, setCadence] = useState<FinanceReportCadence>("monthly");
  const [recipient, setRecipient] = useState("");
  const [nextRunDate, setNextRunDate] = useState(tomorrow());
  const load = useCallback(async () => {
    try {
      setRows(await api.reportSchedules());
      setError(null);
    } catch (reason) {
      setError(
        financeMessage(reason, strings.financeReportSchedulesLoadFailed),
      );
    } finally {
      setLoading(false);
    }
  }, [api]);
  useEffect(() => {
    void load();
  }, [load]);
  async function create() {
    if (!recipient.trim() || !nextRunDate) return;
    setBusy(true);
    try {
      await api.createReportSchedule({
        report,
        cadence,
        recipient: recipient.trim(),
        nextRunDate,
      });
      setRecipient("");
      await load();
    } catch (reason) {
      setError(
        financeMessage(reason, strings.financeReportSchedulesCreateFailed),
      );
    } finally {
      setBusy(false);
    }
  }
  async function remove(row: FinanceReportSchedule) {
    if (
      !(await dialogs.confirm({
        title: strings.financeReportScheduleDeleteTitle,
        message: strings.financeReportScheduleDeleteMessage,
        confirmLabel: strings.delete,
      }))
    )
      return;
    setBusy(true);
    try {
      await api.deleteReportSchedule(row.id);
      await load();
    } catch (reason) {
      setError(
        financeMessage(reason, strings.financeReportScheduleDeleteFailed),
      );
    } finally {
      setBusy(false);
    }
  }
  if (loading)
    return (
      <div className="grid flex-1 place-items-center">
        <Spinner size={22} />
      </div>
    );
  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-5 p-6 max-sm:p-4">
      <section>
        <p className="m-0 text-xs font-semibold uppercase tracking-[0.12em] text-accent">
          {strings.financeReportAutomation}
        </p>
        <h2 className="m-0 mt-1 text-xl font-semibold text-primary">
          {strings.financeReportSchedules}
        </h2>
        <p className="m-0 mt-1 text-sm text-secondary">
          {strings.financeReportSchedulesHint}
        </p>
      </section>
      {error && <ErrorBanner message={error} />}
      <Card pad="none" className="overflow-hidden">
        <div className="grid gap-3 border-b border-subtle bg-raised p-5 lg:grid-cols-[1.2fr_1fr_1.5fr_1fr_auto] lg:items-end">
          <Field label={strings.financeReport}>
            <Select
              value={report}
              onChange={(event) =>
                setReport(event.target.value as FinanceReportKind)
              }
            >
              {REPORTS.map((value) => (
                <option key={value} value={value}>
                  {strings.financeReportKind(value)}
                </option>
              ))}
            </Select>
          </Field>
          <Field label={strings.financeCadence}>
            <Select
              value={cadence}
              onChange={(event) =>
                setCadence(event.target.value as FinanceReportCadence)
              }
            >
              {CADENCES.map((value) => (
                <option key={value} value={value}>
                  {strings.financeCadenceKind(value)}
                </option>
              ))}
            </Select>
          </Field>
          <Field label={strings.financeRecipient}>
            <Input
              type="email"
              value={recipient}
              onChange={(event) => setRecipient(event.target.value)}
              placeholder="finance@example.com"
            />
          </Field>
          <Field label={strings.financeNextDelivery}>
            <DatePicker value={nextRunDate} onChange={setNextRunDate} />
          </Field>
          <Button
            disabled={busy || !recipient.trim() || !nextRunDate}
            onClick={() => void create()}
          >
            <Plus className="size-4" />
            {strings.financeAddSchedule}
          </Button>
        </div>
        <div className="divide-y divide-subtle">
          {rows.length === 0 ? (
            <div className="grid place-items-center gap-2 px-5 py-12 text-center">
              <span className="grid size-11 place-items-center rounded-2xl bg-[var(--accent-soft)] text-accent">
                <CalendarClock className="size-5" />
              </span>
              <p className="m-0 font-medium text-primary">
                {strings.financeNoSchedules}
              </p>
              <p className="m-0 text-sm text-secondary">
                {strings.financeNoSchedulesHint}
              </p>
            </div>
          ) : (
            rows.map((row) => (
              <article
                key={row.id}
                className="flex flex-wrap items-center gap-4 px-5 py-4"
              >
                <span className="grid size-10 place-items-center rounded-xl bg-raised text-secondary">
                  <FileSpreadsheet className="size-4" />
                </span>
                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-semibold text-primary">
                      {strings.financeReportKind(row.report)}
                    </span>
                    <Badge tone="accent">
                      {strings.financeCadenceKind(row.cadence)}
                    </Badge>
                  </div>
                  <p className="m-0 mt-1 text-sm text-secondary">
                    {row.recipient} · {strings.financeNextRun(row.nextRunDate)}
                  </p>
                </div>
                <Button
                  variant="ghost"
                  aria-label={strings.delete}
                  disabled={busy}
                  onClick={() => void remove(row)}
                >
                  <Trash2 className="size-4" />
                  {strings.delete}
                </Button>
              </article>
            ))
          )}
        </div>
      </Card>
    </div>
  );
}
function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label className="grid gap-1.5 text-sm font-medium text-primary">
      {label}
      {children}
    </label>
  );
}
