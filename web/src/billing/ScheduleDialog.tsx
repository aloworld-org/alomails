// Set up or adjust one recurring invoice (B2.11).
//
// Two shapes, one form. Setting one up starts from an **invoice already on
// screen**: that document supplies the customer, the currency, the terms and
// the lines, so this dialog only asks the two things the document cannot know —
// how often, and from when. That is deliberate. A standalone "new recurring
// invoice" form would need a second line editor, and a second line editor is a
// second place for a price to be typed differently from the one on the paper.
//
// Adjusting an existing one asks less again: an arrangement IS its customer,
// currency, terms and start date, so the server does not accept those on a
// `PATCH` and this form does not offer them.
//
// It holds no rules of its own. The name, the cadence, the dates and the range
// a start date may sit in are all judged by the store, and what comes back on a
// refusal is shown as-is.
import { useState } from "react";
import { RefreshCw } from "lucide-react";

import { strings } from "../i18n";
import { billingMessage, useBillingApi } from "./api";
import { CADENCES } from "./cadence";
import { DialogFrame, Field } from "./parts";
import type {
  BillingInvoice,
  BillingScheduleSummary,
  ScheduleCadence,
  ScheduleDraft,
} from "./types";
import styles from "./billingStyles";

interface Props {
  /** The arrangement being adjusted, or `null` when one is being set up. */
  schedule: BillingScheduleSummary | null;
  /** The document the template is copied from. Required to set one up, and
   *  ignored when adjusting: the template of a live arrangement is edited on
   *  the arrangement, not by pointing it at another invoice. */
  from?: BillingInvoice;
  /** A name to start from — the customer's, usually, since the dialog cannot
   *  see the customer list. */
  suggestedName?: string;
  onClose: () => void;
  /** The caller reloads from the server, so the saved record is not passed on. */
  onSaved: (schedule: BillingScheduleSummary) => void;
}

/** Today as `YYYY-MM-DD` in the browser's own zone — only ever a *default* for
 *  the date box. Which occurrences are due is the server's judgement, made
 *  against its own date. */
function todayValue(): string {
  const now = new Date();
  const month = String(now.getMonth() + 1).padStart(2, "0");
  const day = String(now.getDate()).padStart(2, "0");
  return `${now.getFullYear()}-${month}-${day}`;
}

export function ScheduleDialog({ schedule, from, suggestedName, onClose, onSaved }: Props) {
  const api = useBillingApi();
  const [name, setName] = useState(schedule?.name ?? suggestedName ?? "");
  const [cadence, setCadence] = useState<ScheduleCadence>(schedule?.cadence ?? "monthly");
  const [startDate, setStartDate] = useState(schedule?.startDate ?? todayValue());
  const [endDate, setEndDate] = useState(schedule?.endDate ?? "");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function save() {
    setBusy(true);
    setError(null);
    try {
      // A cleared end date is `null` — "keep going" — not an absent field,
      // which would leave the stored one where it was.
      const draft: ScheduleDraft = {
        name: name.trim(),
        cadence,
        endDate: endDate.trim() === "" ? null : endDate.trim(),
      };
      if (schedule === null) {
        if (from === undefined) return;
        onSaved(
          await api.createSchedule({
            ...draft,
            customerId: from.customerId,
            startDate,
            currency: from.currency,
            paymentTermsDays: from.paymentTermsDays,
            reference: from.reference,
            note: from.note,
            // The document's own lines, copied as they stand — the frozen
            // prices, in print order. The server snapshots them again on the
            // arrangement, so a later edit to this invoice changes nothing.
            lines: from.lines.map((line) => ({
              description: line.description,
              unit: line.unit,
              qtyMilli: line.qtyMilli,
              unitPriceCents: line.unitPriceCents,
              vatRateBp: line.vatRateBp,
            })),
          }),
        );
      } else {
        onSaved(await api.updateSchedule(schedule.id, draft));
      }
    } catch (err) {
      // A refusal keeps the form up with everything typed: the point of showing
      // the reason is that it can be fixed in place.
      setError(billingMessage(err, strings.billingSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  const anchorDay = Number(startDate.slice(8, 10));

  return (
    <DialogFrame
      Icon={RefreshCw}
      title={schedule === null ? strings.billingScheduleFrom : strings.billingRecurringTitle}
      subtitle={strings.billingScheduleFromHint}
      error={error}
      busy={busy}
      canSubmit={name.trim() !== "" && startDate !== ""}
      submitLabel={schedule === null ? strings.billingCreate : strings.billingSave}
      onClose={onClose}
      onSubmit={() => void save()}
    >
      <Field label={strings.billingScheduleName} hint={strings.billingScheduleNameHint}>
        <input
          className={styles.input}
          value={name}
          onChange={(e) => setName(e.target.value)}
          autoFocus
          required
        />
      </Field>

      <div className={styles.row}>
        <Field label={strings.billingScheduleCadence}>
          <select
            className={styles.select}
            value={cadence}
            onChange={(e) => setCadence(e.target.value as ScheduleCadence)}
          >
            {CADENCES.map((c) => (
              <option key={c.value} value={c.value}>
                {c.label()}
              </option>
            ))}
          </select>
        </Field>
        {/* The start date is the day the arrangement is anchored to, and an
            arrangement IS its start date — so it is set once and then shown
            read-only rather than offered as an edit the server would ignore. */}
        <Field
          label={strings.billingScheduleStart}
          {...(cadence === "weekly" || Number.isNaN(anchorDay)
            ? {}
            : { hint: strings.billingScheduleAnchorHint(anchorDay) })}
        >
          {schedule === null ? (
            <input
              className={styles.input}
              type="date"
              value={startDate}
              onChange={(e) => setStartDate(e.target.value)}
              required
            />
          ) : (
            <p className={styles.readOnlyValue}>{startDate}</p>
          )}
        </Field>
        <Field label={strings.billingScheduleEnd} hint={strings.billingScheduleEndNever}>
          <input
            className={styles.input}
            type="date"
            value={endDate}
            onChange={(e) => setEndDate(e.target.value)}
          />
        </Field>
      </div>
    </DialogFrame>
  );
}
