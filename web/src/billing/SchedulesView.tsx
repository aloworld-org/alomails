// The recurring-invoice list (B2.11): every standing arrangement the tenant
// bills on a rhythm, what one occurrence of it is worth, when the next one
// falls, and what has already been raised from it.
//
// The one thing this screen keeps saying, in the button, in the notice and in
// the empty state, is that a run raises **drafts**. Nothing here issues a
// numbered document, and a screen that let anyone believe otherwise would be a
// screen nobody trusts with their ledger.
//
// It computes nothing. The amount per occurrence, the next date, whether an
// arrangement is due, whether it has finished — all of them are the server's,
// judged against the server's own date, so a browser with a wrong clock cannot
// invent a due arrangement or hide one.
//
// Arrangements are set up from an invoice, not here: that document supplies the
// customer, the currency, the terms and the lines. The empty state says so, and
// `InvoiceEditor` carries the button.
import { useCallback, useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { RefreshCw } from "lucide-react";

import { Button, Spinner, Table, Td, Th, Toolbar, useDialogs } from "../ds";
import { strings, useLocale } from "../i18n";
import { billingMessage, useBillingApi } from "./api";
import { BillingPagination } from "./BillingPagination";
import { BillingStatusCell } from "./BillingStatusCell";
import { cadenceLabel } from "./cadence";
import { formatDocumentDate } from "./dates";
import { formatAmount } from "./money";
import { ScheduleDialog } from "./ScheduleDialog";
import { ScheduleChips } from "./ScheduleChips";
import { BillingLoading, EmptyState, ErrorBanner } from "./parts";
import type { BillingCustomer, BillingScheduleSummary } from "./types";
import styles from "./billingStyles";
import { useBillingPagination } from "./useBillingPagination";

/** The chips one arrangement wears, in reading order: what it is doing, then
 *  whether it is waiting on a run.
 *
 *  "Finished" and "Paused" are different facts and are never shown together:
 *  one ran out of dates, the other was stopped by a colleague, and a reader has
 *  to be able to tell which. */
export function SchedulesView() {
  const api = useBillingApi();
  const locale = useLocale();
  const navigate = useNavigate();
  const { confirm } = useDialogs();
  const [schedules, setSchedules] = useState<BillingScheduleSummary[]>([]);
  const [customers, setCustomers] = useState<BillingCustomer[]>([]);
  const [editing, setEditing] = useState<BillingScheduleSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [running, setRunning] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Archived customers are included: an arrangement raised for a customer who
  // has since been archived still has to name them.
  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [list, people] = await Promise.all([
        api.schedules(),
        api.customers(true),
      ]);
      setSchedules(list);
      setCustomers(people);
      setError(null);
    } catch (err) {
      setError(billingMessage(err, strings.billingLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api]);

  useEffect(() => {
    void load();
  }, [load]);

  const names = useMemo(
    () => new Map(customers.map((c) => [c.id, c.name] as const)),
    [customers],
  );
  const paged = useBillingPagination(schedules, "schedules");

  /** Raise everything that is due, now rather than at the next hourly run. The
   *  server decides what "due" means; this only reports what appeared. */
  async function runDue() {
    setRunning(true);
    setNotice(null);
    setError(null);
    try {
      const raised = await api.runSchedules();
      setNotice(
        raised.length === 0
          ? strings.billingScheduleRunNone
          : strings.billingScheduleRunDrafted(raised.length),
      );
      await load();
    } catch (err) {
      setError(billingMessage(err, strings.billingActionFailed));
    } finally {
      setRunning(false);
    }
  }

  async function setActive(schedule: BillingScheduleSummary, active: boolean) {
    setError(null);
    try {
      await api.setScheduleActive(schedule.id, active);
      await load();
    } catch (err) {
      setError(billingMessage(err, strings.billingActionFailed));
    }
  }

  /** Only an arrangement that has never raised anything can go. One that has is
   *  refused by the server with the sentence that says to pause it instead —
   *  which is shown as-is rather than being second-guessed here. */
  async function remove(schedule: BillingScheduleSummary) {
    if (
      !(await confirm({
        title: strings.billingScheduleDeleteTitle,
        message: strings.billingScheduleDeleteMessage,
        confirmLabel: strings.billingScheduleDelete,
        danger: true,
      }))
    ) {
      return;
    }
    setError(null);
    try {
      await api.deleteSchedule(schedule.id);
      await load();
    } catch (err) {
      setError(billingMessage(err, strings.billingActionFailed));
    }
  }

  return (
    <div className={styles.page}>
      <Toolbar label={strings.billingRecurringTitle} className={styles.listBar}>
        <h2 className={styles.sectionTitle}>{strings.billingRecurringTitle}</h2>
        {loading && <Spinner size={16} />}
        <Button onClick={() => void runDue()} disabled={running || loading}>
          {strings.billingScheduleRunDue}
        </Button>
      </Toolbar>
      <p className={styles.hint}>{strings.billingScheduleRunHint}</p>

      {error !== null && <ErrorBanner message={error} />}
      {notice !== null && (
        <p className={styles.notice} role="status">
          {notice}
        </p>
      )}

      {loading ? (
        <BillingLoading />
      ) : schedules.length === 0 ? (
        <EmptyState
          Icon={RefreshCw}
          title={strings.billingNoSchedulesTitle}
          body={strings.billingNoSchedulesBody}
          cta={strings.billingInvoices}
          onCta={() => void navigate("../invoices")}
        />
      ) : (
        <><Table
          label={strings.billingRecurringTitle}
          className={styles.listTable}
          stickyHeader
          interactiveRows
        >
          <thead>
            <tr>
              <Th>{strings.billingScheduleName}</Th>
              <Th>{strings.billingColCustomer}</Th>
              <Th>{strings.billingScheduleCadence}</Th>
              <Th>{strings.billingScheduleNext}</Th>
              <Th>{strings.billingColStatus}</Th>
              <Th numeric>{strings.billingScheduleEach}</Th>
              <Th numeric>{strings.billingScheduleRaised}</Th>
              <Th hideLabel>{strings.billingColActions}</Th>
            </tr>
          </thead>
          <tbody>
            {paged.records.map((schedule) => (
              <tr key={schedule.id}>
                <td>
                  <button
                    type="button"
                    className={styles.rowName}
                    onClick={() => setEditing(schedule)}
                  >
                    {schedule.name}
                  </button>
                </td>
                <td>
                  {names.get(schedule.customerId) ??
                    strings.billingUnknownCustomer}
                </td>
                <td>{cadenceLabel(schedule.cadence)}</td>
                {/* The server's date, formatted — never recomputed here. */}
                <td>
                  {schedule.ended
                    ? strings.billingNoDate
                    : formatDocumentDate(
                        schedule.nextRunDate,
                        locale,
                        strings.billingNoDate,
                      )}
                </td>
                <BillingStatusCell>
                  <ScheduleChips schedule={schedule} />
                </BillingStatusCell>
                <Td numeric>
                  {formatAmount(
                    schedule.totals.grossCents,
                    locale,
                    schedule.currency,
                  )}
                </Td>
                <Td numeric>{schedule.raisedCount}</Td>
                <td className={styles.rowActions}>
                  <button
                    type="button"
                    className={styles.linkAction}
                    onClick={() => void setActive(schedule, !schedule.active)}
                  >
                    {schedule.active
                      ? strings.billingSchedulePause
                      : strings.billingScheduleResume}
                  </button>
                  {/* Offered only where it can succeed: an arrangement that
                        has raised documents is paused, not deleted. */}
                  {schedule.raisedCount === 0 && (
                    <button
                      type="button"
                      className={styles.linkAction}
                      onClick={() => void remove(schedule)}
                    >
                      {strings.billingScheduleDelete}
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </Table><BillingPagination {...paged} onPage={paged.setPage} /></>
      )}

      {editing !== null && (
        <ScheduleDialog
          schedule={editing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            void load();
          }}
        />
      )}
    </div>
  );
}
