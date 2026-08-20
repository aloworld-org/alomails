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
  // ---- the catalog half (B5.02/B5.03) ------------------------------------
  // The same rows seen as things rather than as prices. They are on THIS
  // record and not on a sibling one (`docs/design/inventory.md` § The
  // catalog): one product, one table, one editor.
  /** The tenant's own code for the item; empty is legitimate. */
  sku: string;
  /** The code on the box (GTIN), check-digit-validated by the server; empty
   *  for the plenty of stock that has none. */
  barcode: string;
  /** Whether the thing has a quantity at all. `false` is a service, which the
   *  move ledger refuses to move. */
  stocked: boolean;
  /** What we pay for it, in the tenant's own currency. `unitPriceCents` is
   *  what we charge. */
  purchasePriceCents: number;
  /** A Drive node, referenced and never copied; `null` when there is no photo.
   *  Read-only here — this wave ships no picker for it. */
  photoNodeId: string | null;
  /** Who we usually buy it from; `null` when nobody in particular. */
  defaultSupplierId: string | null;
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
  sku?: string;
  barcode?: string;
  stocked?: boolean;
  purchasePriceCents?: number;
  /** `null` takes the default supplier off again — the three-way absent /
   *  `null` / value the server reads (`billing_products.rs`). */
  defaultSupplierId?: string | null;
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
  /** The price-list item this line sells, or absent for a charge in words.
   *  Present on a quote's lines; never on an invoice's. */
  productId?: string | null;
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

/**
 * The exchange rate frozen on a document when it was issued (B1.21).
 *
 * `null` on a draft: the rate belongs to the moment the document became a
 * document. It is the rate that was *applied* — EU VAT Directive art. 91 fixes
 * it at the tax point — so a document reprinted next year still shows the rate
 * it was converted at, whatever the table says today.
 */
export interface DocumentFx {
  /** ISO 4217 the tenant keeps books in — what the amounts are restated into. */
  baseCurrency: string;
  /** Units of the document's currency per one unit of `baseCurrency`, x 10^6.
   *  An integer: a rate that multiplies money never passes through a float. */
  rateMicro: number;
  /** The same rate as the decimal it was published as ("1.1626"), for reading. */
  rate: string;
  /** `YYYY-MM-DD`, the day that rate was published. */
  rateDate: string;
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
  /** The recurring arrangement whose due run raised this draft (B2.11), and
   *  which of its occurrences it is for. Both `null` on a document a colleague
   *  typed, which is how the list tells the two apart without a second read. */
  scheduleId: string | null;
  scheduleDueDate: string | null;
  /** The customer's own reference (their PO number); empty when none. */
  reference: string;
  note: string;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
  totals: DocumentTotals;
  /** The rate frozen at issue, `null` while the document is a draft. */
  fx: DocumentFx | null;
  /** The same money in the tenant's accounting currency, present only when
   *  there is something to restate — a document raised in another currency,
   *  already issued. The server's figures, converted at `fx.rateMicro`; the
   *  browser never converts money. */
  baseTotals?: DocumentTotals;
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
  /**
   * The price-list item this line sells, when it was picked from one.
   *
   * **Only a quote records it**, and it is what decides what accepting the offer
   * raises — goods become a sales order, services a draft invoice. Invoices,
   * bills and recurring templates ignore it: they are money documents, and only
   * an offer can become an order. It never prices anything.
   */
  productId?: string | undefined;
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
  /** ISO 4217 the tenant keeps books in (B1.21). Documents may be raised in any
   *  currency; this is the one the VAT summary and the VAT total printed on a
   *  foreign-currency document are expressed in. Never blank — a tenant that has
   *  said nothing keeps books in euro. */
  baseCurrency: string;
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
  baseCurrency?: string;
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

/**
 * What was billed at one VAT rate in a period (B1.20). The same shape as a
 * document's own [`VatSubtotal`], because it is the sum of those subtotals —
 * a client reads one thing in both places.
 */
export type VatReportRate = VatSubtotal;

/** A period's figures in one currency. Currency groups are never added
 *  together: a document is worth what it says in the currency it was raised in.
 *  What *is* added up, once, is `VatReport.base` (B1.21). */
export interface VatReportCurrency {
  /** ISO 4217 code the documents in this group were raised in. */
  currency: string;
  /** How many ordinary invoices contributed. */
  invoiceCount: number;
  /** How many credit notes contributed — the corrections, which subtract. */
  creditNoteCount: number;
  netCents: number;
  vatCents: number;
  grossCents: number;
  /** One row per rate that appears, ascending. */
  byRate: VatReportRate[];
  /** What this group contributes to `VatReport.base`, each of its documents
   *  converted at the rate frozen on it. Equal to the figures above when the
   *  group is already in the accounting currency. */
  baseNetCents: number;
  baseVatCents: number;
  baseGrossCents: number;
  /** How many documents of this group are in **none** of the base figures
   *  because their rate could not be applied. */
  unconvertedCount: number;
}

/**
 * The whole period in the currency the tenant keeps books in — the figure a VAT
 * return is copied from, each document converted at the rate frozen on it.
 *
 * `unconvertedCount` above zero means the totals here are **incomplete**, and a
 * screen must say so rather than print them plain: a tax figure that is quietly
 * missing a document is the one thing this report must never be.
 */
export interface VatReportBase {
  /** ISO 4217 the tenant keeps books in. */
  currency: string;
  netCents: number;
  vatCents: number;
  grossCents: number;
  /** The per-rate boxes of a return, across every currency, ascending. */
  byRate: VatReportRate[];
  unconvertedCount: number;
}

/**
 * The VAT summary of a period: what was billed at each rate between two days,
 * both included.
 *
 * Every figure is the server's, computed from the documents themselves on each
 * call — only those that stand (`issued` and `paid`), judged on the issue date
 * frozen on them, with credit notes subtracting.
 */
export interface VatReport {
  /** `YYYY-MM-DD`, echoed by the server: a figure copied onto a return has to
   *  say which days it covers. */
  from: string;
  to: string;
  /** One group per currency present; empty when the period holds nothing. */
  currencies: VatReportCurrency[];
  /** The same period in the accounting currency — stated even when the period
   *  is empty, so a report always says what its nothing is nothing in. */
  base: VatReportBase;
}

/**
 * One stored exchange rate: on this day, one euro bought this much of this
 * currency (B1.21).
 *
 * Held per tenant, because a tenant is audited against the file *it* imported.
 * The euro is what the published table quotes against, so it never appears here.
 */
export interface FxRate {
  /** ISO 4217 of the quoted currency, uppercase. */
  currency: string;
  /** `YYYY-MM-DD`, the day it was published — not the day it was imported. */
  date: string;
  /** Micro-units of `currency` per one euro. An integer, never a float. */
  rateMicro: number;
  /** The same rate as the decimal it was published as ("1.1626"). */
  rate: string;
  /** `ecb` for a parsed reference-rate file, `manual` for one entered by hand. */
  source: "ecb" | "manual";
  updatedBy: string;
  updatedAt: string;
}

/** What an import of a published rate file did. `from`/`to` are `null` for a
 *  file with no data rows. */
export interface FxImport {
  rates: number;
  days: number;
  currencies: number;
  from: string | null;
  to: string | null;
}

/** One rate as written by hand. The rate is a **string**, the decimal as
 *  published: a number would be a float, and a float is what makes 1.1626
 *  sometimes 1.16259999. */
export interface FxRateDraft {
  currency: string;
  date: string;
  rate: string;
}

/**
 * The reminder the dunning click wrote into Drafts (B1.26).
 *
 * Every figure in it is the server's, read off the stored invoice: how late the
 * document is, and what is still owed after the payments recorded against it.
 * The browser neither counts days nor sums money — it repeats what came back.
 */
export interface ReminderDraft {
  /** The message id of the draft, in the sender's own Drafts folder. */
  id: string;
  /** The number of the invoice the letter is about. */
  invoice: string;
  /** The customer address it is addressed to. */
  to: string;
  subject: string;
  /** Whole days past the due date; `0` when it is not late yet. */
  daysOverdue: number;
  /** What is still owed, in integer cents of the document's currency. */
  outstandingCents: number;
}

/**
 * How often a recurring arrangement bills (B2.11).
 *
 * Four rhythms, not a general "every N units" rule: these are the ones a
 * business actually bills on, and each is a word a tenant can read.
 */
export type ScheduleCadence = "weekly" | "monthly" | "quarterly" | "yearly";

/** A recurring arrangement as a list entry: the header and what ONE occurrence
 *  of it is worth. It never issues anything — every run raises drafts. */
export interface BillingScheduleSummary {
  id: string;
  customerId: string;
  name: string;
  cadence: ScheduleCadence;
  /** The day of the month it is anchored to (1–31); unused by `weekly`. */
  anchorDay: number;
  /** `YYYY-MM-DD`. The first date it bills on, and the day it is anchored to. */
  startDate: string;
  /** The last date it may bill on, or `null` for "until somebody stops it". */
  endDate: string | null;
  /** The next date a run will raise a draft for. Moved only by a run. */
  nextRunDate: string;
  lastRunDate: string | null;
  /** Paused arrangements keep their dates and resume where they left off. */
  active: boolean;
  /** Computed by the server: it has an end date and has passed it. Not the
   *  same as paused — a reader must be able to tell "finished" from
   *  "stopped". */
  ended: boolean;
  /** Computed by the server against its own date: a run now would raise
   *  something. */
  due: boolean;
  currency: string;
  paymentTermsDays: number;
  reference: string;
  note: string;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
  /** What one occurrence is worth — the server's figures, from the template. */
  totals: DocumentTotals;
  /** How many drafts it has raised so far. */
  raisedCount: number;
}

/** A whole arrangement: the header and the template lines it bills. */
export interface BillingSchedule extends BillingScheduleSummary {
  lines: DocumentLine[];
}

/**
 * The writable parts of an arrangement; absent means "leave as it is".
 *
 * `customerId`, `currency`, `paymentTermsDays` and `startDate` are read only
 * when it is created: an arrangement IS those, and changing one would leave the
 * drafts it already raised explained by a schedule that no longer matches them.
 * `nextRunDate` and `active` are not writable at all — the first moves only by
 * a run, and pausing has its own route.
 */
export interface ScheduleDraft {
  customerId?: string;
  name?: string;
  cadence?: ScheduleCadence;
  startDate?: string;
  /** `null` clears the end date ("keep going"); absent leaves it as it is. */
  endDate?: string | null;
  currency?: string;
  paymentTermsDays?: number;
  reference?: string;
  note?: string;
  lines?: LineDraft[];
}

/**
 * What accepting an offer produced. **Exactly one of the two is present**: an
 * offer naming a price-list item is for goods and becomes a draft sales order,
 * one naming none becomes the draft invoice it always did.
 *
 * Two nullable fields rather than a tagged union because that is what the server
 * sends; a caller should branch on which is present rather than assume either.
 */
export interface QuoteAcceptance {
  quote: BillingQuote;
  invoice: BillingInvoice | null;
  /** The draft order, when the offer was for goods. Only its id is needed here:
   *  following it leaves Billing for the Inventory screen. */
  salesOrder: { id: string } | null;
}
