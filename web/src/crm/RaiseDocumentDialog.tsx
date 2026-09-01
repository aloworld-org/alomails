// Turning a deal into a billing document (alo CRM, B2.08) — the won-deal
// handoff, as one small form.
//
// It asks for exactly what a deal cannot answer on its own, and nothing else:
//
// - **The VAT rate** its single line is billed at, when the deal is worth
//   something. The server refuses to guess one, because a rate on an invoice is
//   a compliance statement and a machine must not make it. Typed as a
//   percentage and sent as basis points, which is how every rate crosses the
//   wire in alo.
// - **The customer's country**, when the deal is still a lead and a customer
//   row is about to be created from it. It decides VAT treatment, so it is
//   asked rather than assumed — and it is not asked at all when the deal
//   already names a customer.
//
// Everything else is the deal's: the company, the contact, the currency, the
// value. And everything the document then becomes is billing's — this screen
// never issues, sends or prices anything, and the answer it shows is the
// server's own draft.
import { useState } from "react";
import { FileText } from "lucide-react";
import { Link } from "react-router-dom";

import { formatAmount, parseHundredths } from "../billing";
import { Field, Input } from "../ds";
import { strings, useLocale } from "../i18n";
import { crmMessage, useCrmApi } from "./api";
import { DialogFrame } from "./parts";
import type { CrmDeal, DocumentKind, RaisedDocument } from "./types";
import styles from "./CrmModule.module.css";

interface Props {
  deal: CrmDeal;
  /** Which document to raise — decided by the button that opened this. */
  kind: DocumentKind;
  onClose: () => void;
  /** The deal as the server answered it: raising a document can give a lead a
   *  customer, so the drawer redraws from this rather than from what it held. */
  onRaised: (deal: CrmDeal) => void;
}

/** What was raised, once it has been: enough to say so and to open it. */
interface Raised {
  kind: DocumentKind;
  document: RaisedDocument;
}

export function RaiseDocumentDialog({ deal, kind, onClose, onRaised }: Props) {
  const api = useCrmApi();
  const locale = useLocale();
  const [rate, setRate] = useState("");
  const [country, setCountry] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [raised, setRaised] = useState<Raised | null>(null);

  // What the server will require of THIS deal: a rate only when there is a line
  // to rate, a country only when a customer is about to be created. Asking for
  // either when it cannot be used would be a form inventing a rule.
  const needsRate = deal.valueCents !== 0;
  const needsCountry = deal.customerId === null;
  // Basis points, always — a rate is never a float on the wire. `null` is "not
  // a number the server would take", which keeps the submit disabled instead of
  // sending something it will refuse.
  const rateBp = needsRate ? parseHundredths(rate) : null;
  const canSubmit =
    (!needsRate || rateBp !== null) &&
    (!needsCountry || country.trim().length === 2);

  async function submit() {
    setBusy(true);
    try {
      const handoff = {
        ...(rateBp === null ? {} : { vatRateBp: rateBp }),
        ...(needsCountry ? { country: country.trim() } : {}),
      };
      const answer =
        kind === "invoice"
          ? await api.raiseInvoice(deal.id, handoff)
          : await api.raiseQuote(deal.id, handoff);
      setRaised({
        kind,
        document: "invoice" in answer ? answer.invoice : answer.quote,
      });
      setError(null);
      onRaised(answer.deal);
    } catch (err) {
      // The server's own sentence when it sent one: it names the rule that was
      // broken ("this deal names no company; give it one…").
      setError(crmMessage(err, strings.crmRaiseFailed));
    } finally {
      setBusy(false);
    }
  }

  if (raised !== null) {
    const path = raised.kind === "invoice" ? "invoices" : "quotes";
    return (
      <DialogFrame
        Icon={FileText}
        title={strings.crmRaisedTitle(strings.crmDocumentDraft(raised.kind))}
        subtitle={strings.crmRaisedSubtitle}
        error={null}
        busy={false}
        canSubmit
        submitLabel={strings.crmClose}
        onClose={onClose}
        onSubmit={onClose}
      >
        <p className={styles.reportBasis}>
          {strings.crmRaisedWorth(
            formatAmount(
              raised.document.totals.grossCents,
              locale,
              raised.document.currency,
            ),
          )}
        </p>
        <Link
          className={styles.linkAction}
          to={`/billing/${path}/${raised.document.id}`}
          onClick={onClose}
        >
          {strings.crmOpenInBilling}
        </Link>
      </DialogFrame>
    );
  }

  return (
    <DialogFrame
      Icon={FileText}
      title={strings.crmRaiseTitle(strings.crmDocumentDraft(kind))}
      subtitle={strings.crmRaiseSubtitle}
      error={error}
      busy={busy}
      canSubmit={canSubmit}
      submitLabel={strings.crmRaiseConfirm}
      onClose={onClose}
      onSubmit={() => void submit()}
    >
      <p className={styles.reportBasis}>
        {strings.crmRaiseFrom(
          deal.title,
          formatAmount(deal.valueCents, locale, deal.currency),
        )}
      </p>
      {needsRate && (
        <Field
          label={strings.crmFieldVatRate}
          hint={strings.crmVatRateHint}
          hintDisplay="tooltip"
          error={
            rate !== "" && rateBp === null ? strings.crmNotAnAmount : undefined
          }
        >
          {(control) => (
            <Input
              {...control}
              inputMode="decimal"
              value={rate}
              onChange={(e) => setRate(e.target.value)}
              placeholder="21"
              autoFocus
            />
          )}
        </Field>
      )}
      {needsCountry && (
        <Field label={strings.crmFieldCountry} hint={strings.crmCountryHint} hintDisplay="tooltip">
          {(control) => (
            <Input
              {...control}
              value={country}
              onChange={(e) => setCountry(e.target.value.toUpperCase())}
              maxLength={2}
              placeholder="DE"
              autoFocus={!needsRate}
            />
          )}
        </Field>
      )}
    </DialogFrame>
  );
}
