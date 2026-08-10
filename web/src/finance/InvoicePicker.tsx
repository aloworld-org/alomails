// Saying by hand which invoice a bank line settled (B4.09c, stage 3).
//
// The two guessing stages before it (`bank_match`, `bank_match_heuristic`)
// propose; this is the door for the line they had nothing to say about — a
// customer who never quoted a number, a transfer that arrived in a different
// month, the payment somebody made for a colleague's company. Without it the
// screen would be a suggestions screen, and a line with no suggestion could
// only ever be set aside.
//
// **It offers only documents that can take money**: issued, unsettled, not a
// credit note (`useOpenInvoices`, Billing's own list). Every one of those is a
// categorical fact the server computed; nothing here works out what is owed.
//
// **The amount is the line's, not a number in a box.** One line settles one
// document in full or in part, and the store compares what this dialog sends
// with what the bank said the line moves — so the person picks *which*
// document, never *how much*, and a stale screen is a refusal instead of a
// payment for the wrong money.
import { useMemo, useState } from "react";
import { FileText } from "lucide-react";

import { formatAmount, useOpenInvoices, type BillingInvoiceSummary } from "../billing";
import { Spinner } from "../ds";
import { getLocale, strings } from "../i18n";
import { amountLabel, dayLabel } from "./format";
import { DialogFrame, ErrorBanner, Field } from "./parts";
import type { BankLine } from "./types";
import styles from "./FinanceModule.module.css";

export function InvoicePicker({
  line,
  busy,
  error,
  onClose,
  onPick,
}: {
  /** The transaction being attributed. Shown in full, because "which invoice is
   *  this?" is a question about the payment as much as about the ledger. */
  line: BankLine;
  busy: boolean;
  /** The server's refusal, verbatim — the dialog stays open on one, since the
   *  next pick is the correction. */
  error: string | null;
  onClose: () => void;
  onPick: (invoice: BillingInvoiceSummary) => void;
}) {
  const { invoices, error: loadError, loading } = useOpenInvoices();
  const [search, setSearch] = useState("");
  const [picked, setPicked] = useState<string | null>(null);

  /** Narrowed by what was typed, matched against the number and the customer's
   *  own reference — the two things printed on a payment. Client-side because
   *  the list is already in hand and a round trip per keystroke would be worse
   *  than a filter over what is on screen. */
  const shown = useMemo(() => {
    const needle = search.trim().toLowerCase();
    if (needle === "") return invoices;
    return invoices.filter((invoice) =>
      [invoice.number ?? "", invoice.reference, invoice.note]
        .join(" ")
        .toLowerCase()
        .includes(needle),
    );
  }, [invoices, search]);

  const chosen = shown.find((invoice) => invoice.id === picked) ?? null;

  return (
    <DialogFrame
      Icon={FileText}
      title={strings.financeBankPickTitle}
      subtitle={strings.financeBankPickSubtitle(amountLabel(line.amountCents, line.currency))}
      error={error}
      busy={busy}
      canSubmit={chosen !== null}
      submitLabel={strings.financeBankConfirmMatch}
      onClose={onClose}
      onSubmit={() => {
        if (chosen !== null) onPick(chosen);
      }}
    >
      <dl className={styles.summary}>
        <div>
          <dt>{strings.financeBankBookedOn}</dt>
          <dd>{dayLabel(line.bookedOn, "—")}</dd>
        </div>
        <div>
          <dt>{strings.financeBankCounterparty}</dt>
          <dd>{line.counterpartyName ?? strings.financeBankNoCounterparty}</dd>
        </div>
        {line.remittance !== null && line.remittance !== "" && (
          <div>
            <dt>{strings.financeBankRemittance}</dt>
            <dd>{line.remittance}</dd>
          </div>
        )}
      </dl>

      {loadError !== null && <ErrorBanner message={loadError} />}

      <Field label={strings.financeBankFindInvoice} hint={strings.financeBankFindInvoiceHint}>
        <input
          className={styles.input}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          autoComplete="off"
        />
      </Field>

      {loading && shown.length === 0 ? (
        <Spinner size={18} />
      ) : shown.length === 0 ? (
        <p className={styles.sectionNote}>{strings.financeBankNoOpenInvoices}</p>
      ) : (
        <ul className={styles.pickList}>
          {shown.map((invoice) => (
            <li key={invoice.id}>
              <label className={styles.pickRow}>
                <input
                  type="radio"
                  name="invoice"
                  value={invoice.id}
                  checked={picked === invoice.id}
                  onChange={() => setPicked(invoice.id)}
                />
                <span className={styles.pickNumber}>
                  {invoice.number ?? strings.financeBankNoNumber}
                  {invoice.reference !== "" && (
                    <span className={styles.subtle}>{invoice.reference}</span>
                  )}
                </span>
                <span className={styles.pickDue}>
                  {dayLabel(invoice.dueDate, "—")}
                  {invoice.overdue && (
                    <span className={styles.declined}>{strings.financeBankOverdue}</span>
                  )}
                </span>
                <span className={styles.pickAmount}>
                  {formatAmount(invoice.settlement.outstandingCents, getLocale(), invoice.currency)}
                  <span className={styles.subtle}>{strings.financeBankStillOwed}</span>
                </span>
              </label>
            </li>
          ))}
        </ul>
      )}
    </DialogFrame>
  );
}
