import { useState } from "react";
import { Banknote } from "lucide-react";
import { Input } from "../ds";
import { strings } from "../i18n";
import { billingMessage, useBillingApi } from "./api";
import { hundredthsToInput, parseHundredths } from "./money";
import { DialogFrame, Field } from "./parts";
import type { BillingInvoice } from "./types";
import styles from "./billingStyles";

export function PaymentDialog({ invoiceId, currency, outstandingCents, onClose, onRecorded }: { invoiceId: string; currency: string; outstandingCents: number; onClose: () => void; onRecorded: (invoice: BillingInvoice) => void }) {
  const api = useBillingApi();
  const [amount, setAmount] = useState(outstandingCents > 0 ? hundredthsToInput(outstandingCents) : "");
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
      const recorded = await api.recordPayment(invoiceId, { amountCents, ...(paidOn === "" ? {} : { paidOn }), method, reference });
      onRecorded(recorded.invoice);
    } catch (err) {
      setError(billingMessage(err, strings.billingActionFailed));
    } finally {
      setBusy(false);
    }
  }

  return <DialogFrame Icon={Banknote} title={strings.billingRecordPayment} subtitle={strings.billingRecordPaymentHint} error={error} busy={busy} canSubmit={amountCents !== null} submitLabel={strings.billingRecordPayment} onClose={onClose} onSubmit={() => void submit()}>
    <div className={styles.row}>
      <Field label={strings.billingFieldAmount(currency)} hint={strings.billingFieldAmountHint} error={amount !== "" && amountCents === null ? strings.billingNotAnAmount : undefined}>
        <Input inputMode="decimal" value={amount} onChange={(event) => setAmount(event.target.value)} invalid={amount !== "" && amountCents === null} />
      </Field>
      <Field label={strings.billingFieldPaidOn} hint={strings.billingFieldPaidOnHint}>
        <Input type="date" value={paidOn} onChange={(event) => setPaidOn(event.target.value)} />
      </Field>
    </div>
    <Field label={strings.billingFieldMethod} hint={strings.billingFieldMethodHint}>
      <Input value={method} onChange={(event) => setMethod(event.target.value)} placeholder={strings.billingMethodPlaceholder} />
    </Field>
    <Field label={strings.billingFieldPaymentReference} hint={strings.billingFieldPaymentRefHint}>
      <Input value={reference} onChange={(event) => setReference(event.target.value)} />
    </Field>
  </DialogFrame>;
}
