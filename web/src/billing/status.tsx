// How a document's state is shown — one place, so a chip in the list and a
// chip on the editor can never say different things about the same invoice.
//
// The state a reader cares about is not exactly the stored one: an issued
// invoice past its due date is "overdue", which the server computes against
// its own date (`overdue` on every invoice response) rather than the browser's.
// So the chip a row shows is derived from the pair, never from `status` alone.
import { Badge, type BadgeProps } from "../ds";
import { strings } from "../i18n";
import type {
  BillingInvoiceSummary,
  BillingQuoteSummary,
  InvoiceStatus,
  QuoteStatus,
} from "./types";

/** The visual weight of a chip. Named after what it means, not its colour, so
 *  a theme can restyle it without renaming anything. */
export type ChipTone = "neutral" | "info" | "good" | "warn" | "muted";

/** How billing's five meanings are drawn by the design system's four tones
 *  (D2.06b).
 *
 *  `neutral` and `muted` land on the same one, and that is a reconciliation
 *  rather than a loss. Billing drew them a single step apart — `--text-secondary`
 *  against `--text-tertiary` on the same grey pill — which is not a difference
 *  anybody reads, and `Badge`'s own rule is that a tone is never the only
 *  signal: a draft says "Draft" and a cancelled document says "Void". The two
 *  names stay because the *meaning* they carry is still two things, and
 *  `statusTone` is where that distinction is stated. */
const BADGE_TONE = {
  neutral: "neutral",
  info: "accent",
  good: "success",
  warn: "danger",
  muted: "neutral",
} as const satisfies Record<ChipTone, NonNullable<BadgeProps["tone"]>>;

/** A small state label.
 *
 *  It is a `Badge` and not a `Chip`: the design system's distinction is that a
 *  badge is read and a chip is acted on, and nothing here is pressable. The
 *  name is kept because it is what three files call it and what the product
 *  calls the thing on screen. */
export function StatusChip({ tone, label }: { tone: ChipTone; label: string }) {
  return <Badge tone={BADGE_TONE[tone]}>{label}</Badge>;
}

/** What to call a status. An unknown one — a state added to the server before
 *  this client knows it — is shown verbatim rather than blanked. */
export function statusLabel(status: InvoiceStatus): string {
  switch (status) {
    case "draft":
      return strings.billingStatusDraft;
    case "issued":
      return strings.billingStatusIssued;
    case "paid":
      return strings.billingStatusPaid;
    case "void":
      return strings.billingStatusVoid;
    default:
      return status;
  }
}

/** How loudly a status reads: a draft is quiet, money in is good, a cancelled
 *  document is greyed out. */
export function statusTone(status: InvoiceStatus): ChipTone {
  switch (status) {
    case "issued":
      return "info";
    case "paid":
      return "good";
    case "void":
      return "muted";
    default:
      return "neutral";
  }
}

/** The chips one document wears, in reading order: what it is, then what is
 *  wrong with it. A credit note says so first — it is the one thing about a
 *  document a reader must not miss, because its totals are negative. */
export function DocumentChips({ invoice }: { invoice: BillingInvoiceSummary }) {
  return (
    <>
      {invoice.creditNote && (
        <StatusChip tone="warn" label={strings.billingCreditNote} />
      )}
      <StatusChip
        tone={statusTone(invoice.status)}
        label={statusLabel(invoice.status)}
      />
      {invoice.overdue && (
        <StatusChip tone="warn" label={strings.billingStatusOverdue} />
      )}
      {/* Where the document came from, when a standing arrangement raised it
          (B2.11). Quiet on purpose — it is provenance, not a state — but it is
          the one thing that explains why a draft nobody typed is sitting in the
          list, and the reason it reads "Recurring" rather than "Automatic" is
          that nothing about it was: it still has to be issued by a person. */}
      {invoice.scheduleId !== null && (
        <StatusChip tone="info" label={strings.billingRecurringChip} />
      )}
    </>
  );
}

/** What to call a quote's state. */
export function quoteStatusLabel(status: QuoteStatus): string {
  switch (status) {
    case "draft":
      return strings.billingStatusDraft;
    case "sent":
      return strings.billingQuoteStatusSent;
    case "accepted":
      return strings.billingQuoteStatusAccepted;
    case "declined":
      return strings.billingQuoteStatusDeclined;
    case "expired":
      return strings.billingQuoteStatusExpired;
    default:
      return status;
  }
}

/** How loudly an offer's state reads: an open one is the one to look at, a
 *  won one is good, and the two that closed without business are greyed. */
export function quoteStatusTone(status: QuoteStatus): ChipTone {
  switch (status) {
    case "sent":
      return "info";
    case "accepted":
      return "good";
    case "declined":
    case "expired":
      return "muted";
    default:
      return "neutral";
  }
}

/**
 * The chips one offer wears: what it is, then whether it has lapsed.
 *
 * "Lapsed" is the server's computed `expired` flag — the validity date has
 * passed — and it is deliberately worded differently from the `expired`
 * *status*, which is somebody's decision to stop chasing the offer. Only an
 * open offer can lapse; on a closed one the flag says nothing a reader needs.
 */
export function QuoteChips({ quote }: { quote: BillingQuoteSummary }) {
  return (
    <>
      <StatusChip
        tone={quoteStatusTone(quote.status)}
        label={quoteStatusLabel(quote.status)}
      />
      {quote.status === "sent" && quote.expired && (
        <StatusChip tone="warn" label={strings.billingQuoteLapsed} />
      )}
    </>
  );
}
