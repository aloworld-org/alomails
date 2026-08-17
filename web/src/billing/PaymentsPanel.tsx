// The money received against one invoice: what has arrived, what is left, and
// the form that records the next instalment (alo Billing, B1.19).
//
// Three things it deliberately does not do.
//
//   - **It never sums money.** `grossCents`, `paidCents` and `outstandingCents`
//     all come from the server on every read; this panel renders them. The one
//     number it turns into cents is the one the user typed, and that goes to
//     the server to be ruled on.
//   - **It does not decide what a payment may settle.** A draft, a void
//     document and a credit note are refused by the store with a `409` naming
//     which case it is; the panel is simply not offered on them, and the
//     refusal is still shown if the document changed underneath.
//   - **It does not edit a payment.** A payment is a fact that happened: a
//     mis-keyed one is removed and re-entered, which is why the only actions
//     here are "record" and "remove".
//
// Recording or removing one changes the document's status — the server
// projects `paid` from this ledger — so both hand the returned invoice back to
// the editor rather than letting the two screens drift a read apart.
import { useCallback, useEffect, useState } from "react";
import { Banknote } from "lucide-react";

import { Button, Spinner } from "../ds";
import { strings, useLocale } from "../i18n";
import { billingMessage, useBillingApi } from "./api";
import { formatDocumentDate } from "./dates";
import { formatAmount, hundredthsToInput, parseHundredths } from "./money";
import { DialogFrame, ErrorBanner, Field } from "./parts";
import { StatusChip, type ChipTone } from "./status";
import type { BillingInvoice, BillingPayment, DocumentSettlement, PaymentState } from "./types";
import styles from "./billingStyles";

/** What to call a settlement state. An unknown one — a state added to the
 *  server before this client knows it — is shown verbatim rather than blanked. */
function settlementLabel(state: PaymentState): string {
  switch (state) {
    case "unpaid":
      return strings.billingPaymentUnpaid;
    case "partiallyPaid":
      return strings.billingPaymentPartiallyPaid;
    case "paid":
      return strings.billingPaymentPaid;
    default:
      return state;
  }
}

/** How loudly a settlement state reads: settled is good, part-way is worth
 *  looking at, nothing yet is quiet. */
function settlementTone(state: PaymentState): ChipTone {
  switch (state) {
    case "paid":
      return "good";
    case "partiallyPaid":
      return "info";
    default:
      return "neutral";
  }
}

/**
 * The payments section of an invoice.
 *
 * `settlement` comes from the document the editor is already holding, so the
 * figures under the lines and the figures here are the same read; the ledger
 * rows are fetched separately, because a document's screen needs them and its
 * print view does not.
 */
export function PaymentsPanel({
  invoiceId,
  currency,
  settlement,
  onInvoiceChanged,
}: {
  invoiceId: string;
  /** The document's own currency — payments are in it, by construction. */
  currency: string;
  settlement: DocumentSettlement;
  /** Hands back the document the server answered with, so the editor shows the
   *  status this ledger just projected without a second read. */
  onInvoiceChanged: (invoice: BillingInvoice) => void;
}) {
  const api = useBillingApi();
  const locale = useLocale();
  const [payments, setPayments] = useState<BillingPayment[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [recording, setRecording] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const ledger = await api.payments(invoiceId);
      setPayments(ledger.payments);
      setError(null);
    } catch (err) {
      setError(billingMessage(err, strings.billingLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, invoiceId]);

  useEffect(() => {
    void load();
  }, [load]);

  async function remove(paymentId: string) {
    try {
      onInvoiceChanged(await api.deletePayment(invoiceId, paymentId));
      setError(null);
      await load();
    } catch (err) {
      setError(billingMessage(err, strings.billingActionFailed));
    }
  }

  return (
    <section className={styles.lines}>
      <div className={styles.linesHead}>
        <h2 className={styles.sectionTitle}>{strings.billingPayments}</h2>
        {loading && <Spinner size={16} />}
        <StatusChip
          tone={settlementTone(settlement.state)}
          label={settlementLabel(settlement.state)}
        />
        <Button onClick={() => setRecording(true)}>{strings.billingRecordPayment}</Button>
      </div>

      {error !== null && <ErrorBanner message={error} />}

      <dl className={styles.totalsList}>
        <div className={styles.totalsRow}>
          <dt>{strings.billingPaidToDate}</dt>
          <dd>{formatAmount(settlement.paidCents, locale, currency)}</dd>
        </div>
        <div className={`${styles.totalsRow} ${styles.totalsGross}`}>
          <dt>{strings.billingOutstanding}</dt>
          <dd>{formatAmount(settlement.outstandingCents, locale, currency)}</dd>
        </div>
      </dl>
      {settlement.outstandingCents < 0 && (
        <p className={styles.totalsNote}>{strings.billingOverpaidNote}</p>
      )}

      {payments.length === 0 ? (
        !loading && <p className={styles.noMatches}>{strings.billingNoPayments}</p>
      ) : (
        <div className={styles.tableWrap}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th scope="col">{strings.billingColPaidOn}</th>
                <th scope="col">{strings.billingColMethod}</th>
                <th scope="col">{strings.billingColPaymentReference}</th>
                <th scope="col" className={styles.numeric}>
                  {strings.billingColAmount}
                </th>
                <th scope="col">
                  <span className={styles.srOnly}>{strings.billingColActions}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {payments.map((payment) => (
                <tr key={payment.id}>
                  <td>{formatDocumentDate(payment.paidOn, locale, strings.billingNoDate)}</td>
                  <td>{payment.method === "" ? strings.billingNoDate : payment.method}</td>
                  <td className={styles.mono}>
                    {payment.reference === "" ? strings.billingNoDate : payment.reference}
                  </td>
                  <td className={styles.numeric}>
                    {formatAmount(payment.amountCents, locale, currency)}
                  </td>
                  <td className={styles.rowActions}>
                    <button
                      type="button"
                      className={styles.linkAction}
                      onClick={() => void remove(payment.id)}
                    >
                      {strings.billingRemovePayment}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {recording && (
        <PaymentDialog
          invoiceId={invoiceId}
          currency={currency}
          outstandingCents={settlement.outstandingCents}
          onClose={() => setRecording(false)}
          onRecorded={(invoice) => {
            setRecording(false);
            onInvoiceChanged(invoice);
            void load();
          }}
        />
      )}
    </section>
  );
}

/**
 * The record-a-payment form.
 *
 * The amount starts at what is still outstanding, because settling a bill in
 * full is what usually happens and a figure the user only has to confirm is one
 * they cannot mistype. It is still theirs to change — a part payment is the
 * whole reason this ledger exists.
 *
 * The date box starts **empty**, meaning today according to the *server*: a
 * blank is sent as an absent `paidOn`, and the server dates it from its own
 * clock rather than the browser's.
 */
function PaymentDialog({
  invoiceId,
  currency,
  outstandingCents,
  onClose,
  onRecorded,
}: {
  invoiceId: string;
  currency: string;
  outstandingCents: number;
  onClose: () => void;
  onRecorded: (invoice: BillingInvoice) => void;
}) {
  const api = useBillingApi();
  const [amount, setAmount] = useState(
    outstandingCents > 0 ? hundredthsToInput(outstandingCents) : "",
  );
  const [paidOn, setPaidOn] = useState("");
  const [method, setMethod] = useState("");
  const [reference, setReference] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const amountCents = parseHundredths(amount);

  async function submit() {
    if (amountCents === null) return;
    setBusy(true);
    try {
      const recorded = await api.recordPayment(invoiceId, {
        amountCents,
        ...(paidOn === "" ? {} : { paidOn }),
        method,
        reference,
      });
      onRecorded(recorded.invoice);
    } catch (err) {
      setError(billingMessage(err, strings.billingActionFailed));
    } finally {
      setBusy(false);
    }
  }

  return (
    <DialogFrame
      Icon={Banknote}
      title={strings.billingRecordPayment}
      subtitle={strings.billingRecordPaymentHint}
      error={error}
      busy={busy}
      canSubmit={amountCents !== null}
      submitLabel={strings.billingRecordPayment}
      onClose={onClose}
      onSubmit={() => void submit()}
    >
      <div className={styles.row}>
        <Field
          label={strings.billingFieldAmount(currency)}
          hint={strings.billingFieldAmountHint}
          error={amount !== "" && amountCents === null ? strings.billingNotAnAmount : undefined}
        >
          <input
            className={styles.input}
            inputMode="decimal"
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
            aria-invalid={amount !== "" && amountCents === null}
          />
        </Field>
        <Field label={strings.billingFieldPaidOn} hint={strings.billingFieldPaidOnHint}>
          <input
            className={styles.input}
            type="date"
            value={paidOn}
            onChange={(e) => setPaidOn(e.target.value)}
          />
        </Field>
      </div>
      <Field label={strings.billingFieldMethod} hint={strings.billingFieldMethodHint}>
        <input
          className={styles.input}
          value={method}
          onChange={(e) => setMethod(e.target.value)}
          placeholder={strings.billingMethodPlaceholder}
        />
      </Field>
      <Field label={strings.billingFieldPaymentReference} hint={strings.billingFieldPaymentRefHint}>
        <input
          className={styles.input}
          value={reference}
          onChange={(e) => setReference(e.target.value)}
        />
      </Field>
    </DialogFrame>
  );
}
