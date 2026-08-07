// The JSON shapes the `/billing` HTTP surface speaks (alo Billing, ADR 0035,
// wave B1). These mirror `products/mail/alo-jmap/src/billing_customers.rs`,
// `…/billing_products.rs`, `…/billing_invoices.rs` and `…/billing_document.rs`
// field for field — the server is the contract, this file only names it for
// TypeScript.
//
// Two rules the types encode, because they are the ones a UI gets wrong:
//   - **Money is integer cents and rates are basis points.** `unitPriceCents`
//     and `vatRateBp` are whole numbers; a decimal sent here is a `400` from
//     the server, never a silently rounded price.
//   - **A draft is a partial write.** Every field of `CustomerDraft` /
//     `ProductDraft` is optional: an absent field keeps its stored value on a
//     `PATCH`, and takes the server's default on a create. `null` clears a
//     nullable field — which is why the nullable fields are `string | null`
//     rather than merely optional.

/** A billing customer as the server stores it. */
export interface BillingCustomer {
  id: string;
  name: string;
  addressLine1: string;
  addressLine2: string;
  postalCode: string;
  city: string;
  /** ISO 3166-1 alpha-2, uppercase — canonicalised by the server. */
  country: string;
  /** VAT identification number in canonical form; `null` for B2C. */
  vatId: string | null;
  email: string | null;
  paymentTermsDays: number;
  /** ISO 4217, uppercase. */
  currency: string;
  /** Linked address-book contact, if any. */
  contactId: string | null;
  archived: boolean;
  archivedAt: string | null;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
}

/** The writable fields of a customer; absent means "leave as it is". */
export interface CustomerDraft {
  name?: string;
  addressLine1?: string;
  addressLine2?: string;
  postalCode?: string;
  city?: string;
  country?: string;
  vatId?: string | null;
  email?: string | null;
  paymentTermsDays?: number;
  currency?: string;
  contactId?: string | null;
}

/** A price-list item as the server stores it. No currency: a price list is
 *  quoted in the tenant's own currency, and a document carries the currency it
 *  was raised in (`docs/design/billing.md`). */
export interface BillingProduct {
  id: string;
  name: string;
  /** Unit label; empty for a unitless item. */
  unit: string;
  unitPriceCents: number;
  vatRateBp: number;
  archived: boolean;
  archivedAt: string | null;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
}

/** The writable fields of a product; absent means "leave as it is". */
export interface ProductDraft {
  name?: string;
  unit?: string;
  unitPriceCents?: number;
  vatRateBp?: number;
}

/** Where an invoice is in its life. Only a `draft` is editable; the other
 *  three carry a number and are frozen (`docs/design/billing.md`). */
export type InvoiceStatus = "draft" | "issued" | "paid" | "void";

/** A stored line of a document. `netCents` is the server's — quantity times
 *  price, rounded once — and there is deliberately no per-line VAT: VAT is
 *  rounded per rate subtotal, so a per-line column would not add up. */
export interface DocumentLine {
  id: string;
  description: string;
  /** Unit label; empty for a unitless line. */
  unit: string;
  /** Quantity in milli-units (1.5 hours = 1500). Negative on a discount line. */
  qtyMilli: number;
  unitPriceCents: number;
  vatRateBp: number;
  netCents: number;
}

/** One rate's share of a document: what was sold at it, and the VAT on that. */
export interface VatSubtotal {
  rateBp: number;
  netCents: number;
  vatCents: number;
}

/** What a document is worth, in integer cents. Always the server's figures —
 *  nothing here is ever computed in the browser. */
export interface DocumentTotals {
  netCents: number;
  vatCents: number;
  grossCents: number;
  /** The VAT breakdown, one entry per rate that appears on the document. */
  vatByRate: VatSubtotal[];
}

/**
 * Where a document stands against the money that has arrived for it
 * (`platform/alo-store/src/billing_payments.rs`).
 *
 * `partiallyPaid` is not a status: the document is still `issued`, still owed
 * and still overdue when its date passes. It is a fact about money.
 */
export type PaymentState = "unpaid" | "partiallyPaid" | "paid";

/** What a document is worth, what has arrived, and what is left — all integer
 *  cents, all the server's. `outstandingCents` is negative on an overpayment,
 *  which is the figure a refund starts from. */
export interface DocumentSettlement {
  grossCents: number;
  paidCents: number;
  outstandingCents: number;
  state: PaymentState;
}

/** One payment received against an invoice. A fact that happened: it is
 *  removed and re-entered, never edited, so there is no payment draft type for
 *  an update. */
export interface BillingPayment {
  id: string;
  invoiceId: string;
  /** `YYYY-MM-DD`, the day the money arrived as the bank states it — not the
   *  day it was keyed in, which is `createdAt`. */
  paidOn: string;
  /** Integer cents, always positive. */
  amountCents: number;
  /** How it arrived: free text ("bank transfer", "SEPA direct debit", …). */
  method: string;
  /** The bank's own reference for the movement. */
  reference: string;
  createdBy: string;
  createdAt: string;
}

/** A payment as sent to the server. An absent `paidOn` means today according
 *  to the **server**, which is the only date a form that has not asked should
 *  imply. */
export interface PaymentDraft {
  paidOn?: string;
  amountCents: number;
  method?: string;
  reference?: string;
}

/** An invoice as a list entry: the header and what it is worth, no lines. */
export interface BillingInvoiceSummary {
  id: string;
  customerId: string;
  status: InvoiceStatus;
  /** ISO 4217, uppercase — snapshotted from the customer when raised. */
  currency: string;
  /** The legal number, or `null` while the document is still a draft. */
  number: string | null;
  /** `YYYY-MM-DD`, or `null` on a draft. */
  issueDate: string | null;
  dueDate: string | null;
  paymentTermsDays: number;
  /** Computed by the server against its own date, never stored. */
  overdue: boolean;
  creditNote: boolean;
  creditsInvoiceId: string | null;
  quoteId: string | null;
  /** The customer's own reference (their PO number); empty when none. */
  reference: string;
  note: string;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
  totals: DocumentTotals;
  /** Computed on every read from the lines and the payment rows; stored
   *  nowhere, so a list entry and the document can never disagree. */
  settlement: DocumentSettlement;
}

/** A whole invoice: the header, its lines in print order, and its totals. */
export interface BillingInvoice extends BillingInvoiceSummary {
  lines: DocumentLine[];
}

/** A line as sent to the server. The whole set is always written at once, in
 *  print order, so a line carries no id and no position on the way in. */
export interface LineDraft {
  description: string;
  unit: string;
  qtyMilli: number;
  unitPriceCents: number;
  vatRateBp: number;
}

/**
 * The writable parts of an invoice; absent means "leave as it is".
 *
 * There is no `status`, `number`, `issueDate` or `dueDate`: those move only
 * through the lifecycle routes, and a document whose number a client could set
 * is not a document a tax authority would accept. `lines` absent leaves the
 * stored lines alone; `[]` empties the draft.
 */
export interface InvoiceDraft {
  customerId?: string;
  currency?: string;
  paymentTermsDays?: number;
  reference?: string;
  note?: string;
  lines?: LineDraft[];
}

/**
 * Where an offer is in its life. Only a `draft` is editable; `sent` carries a
 * number and is frozen, and the last three are decisions that closed it
 * (`docs/design/billing.md`).
 *
 * `expired` here is the *status* — somebody stopped chasing the offer — which
 * is not the same thing as `BillingQuoteSummary.expired`, the computed flag
 * saying the validity date has passed. A lapsed offer can still be accepted.
 */
export type QuoteStatus = "draft" | "sent" | "accepted" | "declined" | "expired";

/** A quote as a list entry: the header and what it is worth, no lines. */
export interface BillingQuoteSummary {
  id: string;
  customerId: string;
  status: QuoteStatus;
  /** ISO 4217, uppercase — snapshotted from the customer when raised. */
  currency: string;
  /** The offer's number, or `null` while it is still a draft. */
  number: string | null;
  /** `YYYY-MM-DD`, or `null` while the offer has not been sent. */
  sentDate: string | null;
  validUntil: string | null;
  /** How long the offer stands from the day it is sent. */
  validDays: number;
  /** The day the offer was answered, or `null` while it is open. */
  decidedDate: string | null;
  /** Computed by the server against its own date: the validity has passed.
   *  Never stored — a stored flag would be wrong every midnight. */
  expired: boolean;
  /** The customer's own reference (their RFQ number); empty when none. */
  reference: string;
  note: string;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
  totals: DocumentTotals;
}

/** A whole quote: the header, its lines in print order, and its totals. */
export interface BillingQuote extends BillingQuoteSummary {
  lines: DocumentLine[];
}

/**
 * Who the tenant invoices *as* — the issuer side of every printed document.
 *
 * One record per tenant, so it has no id. A tenant that has never saved reads
 * the blanks with `stated: false` rather than a `404`: the record always
 * conceptually exists (`docs/design/billing.md`).
 */
export interface BillingSettings {
  /** `false` until the tenant has saved once; every blank below is then "not
   *  yet said" rather than "deliberately empty". */
  stated: boolean;
  legalName: string;
  addressLine1: string;
  addressLine2: string;
  postalCode: string;
  city: string;
  /** ISO 3166-1 alpha-2, uppercase; blank while unstated. */
  country: string;
  /** Canonical prefixed form; `null` for a tenant not VAT-registered. */
  vatId: string | null;
  /** Company-register number as printed (KVK, SIREN, HRB, …). */
  registrationNo: string;
  email: string;
  phone: string;
  website: string;
  /** Compacted and uppercase, checked against its country length and mod-97. */
  iban: string | null;
  bic: string | null;
  bankName: string;
  /** Printed when the account is not held in the legal name. */
  accountHolder: string;
  /** A line under the totals of every document. */
  footerNote: string;
  updatedBy: string | null;
  updatedAt: string | null;
}

/** The writable fields of the issuer identity; absent means "leave as it is",
 *  and `null` clears a nullable one. */
export interface SettingsDraft {
  legalName?: string;
  addressLine1?: string;
  addressLine2?: string;
  postalCode?: string;
  city?: string;
  country?: string;
  vatId?: string | null;
  registrationNo?: string;
  email?: string;
  phone?: string;
  website?: string;
  iban?: string | null;
  bic?: string | null;
  bankName?: string;
  accountHolder?: string;
  footerNote?: string;
}

/**
 * The writable parts of a quote; absent means "leave as it is".
 *
 * As on an invoice there is no `status`, `number`, `sentDate`, `validUntil` or
 * `decidedDate`: those move only through the lifecycle routes.
 */
export interface QuoteDraft {
  customerId?: string;
  currency?: string;
  validDays?: number;
  reference?: string;
  note?: string;
  lines?: LineDraft[];
}
