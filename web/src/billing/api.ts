// The client for the `/billing` HTTP surface (alo Billing, ADR 0035, wave B1).
//
// Deliberately its own small client rather than more methods on `JmapClient`:
// billing is a plain REST surface with none of JMAP's session, capabilities or
// method-call envelope, and it changes for entirely different reasons than
// mail does. It uses the same authenticated fetch (bearer + refresh handled by
// the auth layer), so there is one session, not two.
//
// It holds NO validation. Name, country, currency, VAT id, price and rate are
// all ruled on by the store, which the billing agent (B1.25) also calls
// directly; a second, weaker copy of those rules here is exactly how the two
// doors end up disagreeing. The form's job is to send what was typed and show
// what came back.
import { useMemo } from "react";

import { useAuth } from "../auth";
import { getLocale } from "../i18n";
import { API_BASE } from "../platform/runtime";
import { RestError, problemDetail, restMessage } from "../platform/rest";
import type {
  BillingCustomer,
  BillingInvoice,
  BillingInvoiceSummary,
  BillingPayment,
  BillingProduct,
  BillingQuote,
  BillingQuoteSummary,
  BillingSchedule,
  BillingScheduleSummary,
  BillingSettings,
  CustomerDraft,
  DocumentSettlement,
  FxImport,
  FxRate,
  FxRateDraft,
  InvoiceDraft,
  InvoiceStatus,
  PaymentDraft,
  ProductDraft,
  QuoteAcceptance,
  QuoteDraft,
  QuoteStatus,
  ScheduleDraft,
  ReminderDraft,
  SettingsDraft,
  VatReport,
} from "./types";

type AuthorizedFetch = (input: string, init?: RequestInit) => Promise<Response>;

/**
 * A failed billing request. `detail` is the server's own sentence when it sent
 * one — the store authors those messages to name the rule that was broken and
 * never to echo stored data, so they are safe to put in front of a user.
 * `status` lets a caller tell "you typed something impossible" (422) from
 * "that record is gone" (404) without parsing prose.
 *
 * The failure shape itself lives in `platform/rest` now that a second business
 * module answers the same `Problem` bodies (B2.07): one implementation, so the
 * two surfaces cannot drift about how a server sentence reaches a user.
 */
export class BillingError extends RestError {
  constructor(status: number, detail: string | null) {
    super(status, detail, "BillingError");
  }
}

/**
 * What to show a user about a failed request: the server's own sentence when
 * it sent one, and `fallback` otherwise (a dropped connection, or a failure
 * whose reason is not the user's business). One helper, so every billing
 * screen reports a failure the same way.
 */
export function billingMessage(error: unknown, fallback: string): string {
  return restMessage(error, fallback);
}

/** The query string naming a reporting period. The two days are sent exactly
 *  as the server spells them (`YYYY-MM-DD`); a blank one is sent as a blank and
 *  refused there, so the rule lives in one place. */
function period(from: string, to: string): string {
  return `from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}`;
}

/**
 * `?lang=` for the routes the server writes prose in: the printable document,
 * and the reminder letter it drafts (B1.27).
 *
 * Read at call time rather than captured, so switching language and printing
 * again gives the other document — and it is the *interface* language, because
 * the person clicking Print is the person who chose it. A per-customer
 * document language is a real want and a different feature: it belongs on the
 * customer record, not on the button.
 *
 * Every other route is left alone. They answer with data, and their refusals
 * are the server's own sentences, which are not translated yet — sending a
 * `lang` there would promise something the server does not do.
 */
function langQuery(): string {
  return `?lang=${encodeURIComponent(getLocale())}`;
}

/** The tenant's billing records — customers, price list, invoices and quotes —
 *  and the lifecycle transitions over them. One instance per auth context. */
export class BillingApi {
  readonly #fetch: AuthorizedFetch;

  constructor(authorizedFetch: AuthorizedFetch) {
    this.#fetch = authorizedFetch;
  }

  /** The customer list, active first; archived only when asked for. */
  customers(includeArchived = false): Promise<BillingCustomer[]> {
    return this.#read<{ customers?: BillingCustomer[] }>(
      `/billing/customers${includeArchived ? "?includeArchived=1" : ""}`,
    ).then((r) => r.customers ?? []);
  }

  /** Creates a customer; answers the STORED record, which is canonicalised
   *  (country and currency uppercased, VAT id compacted and prefixed). */
  createCustomer(draft: CustomerDraft): Promise<BillingCustomer> {
    return this.#write<{ customer: BillingCustomer }>("POST", "/billing/customers", draft).then(
      (r) => r.customer,
    );
  }

  /** Edits a customer. Absent fields keep their stored value; `null` clears a
   *  nullable one. Last writer wins — there is no `If-Match` yet. */
  updateCustomer(id: string, draft: CustomerDraft): Promise<BillingCustomer> {
    return this.#write<{ customer: BillingCustomer }>(
      "PATCH",
      `/billing/customers/${encodeURIComponent(id)}`,
      draft,
    ).then((r) => r.customer);
  }

  /** Archives or restores a customer. Archiving is the only removal: an issued
   *  invoice must always be able to name who it was for. */
  setCustomerArchived(id: string, archived: boolean): Promise<BillingCustomer> {
    return this.#write<{ customer: BillingCustomer }>(
      "POST",
      `/billing/customers/${encodeURIComponent(id)}/archive`,
      { archived },
    ).then((r) => r.customer);
  }

  /** The price list, active first; archived only when asked for. */
  products(includeArchived = false): Promise<BillingProduct[]> {
    return this.#read<{ products?: BillingProduct[] }>(
      `/billing/products${includeArchived ? "?includeArchived=1" : ""}`,
    ).then((r) => r.products ?? []);
  }

  /** Proposes price rows from an image through the tenant's configured AI.
   *  Nothing is written until the user reviews and imports the returned rows. */
  extractPriceListImage(dataUrl: string): Promise<Array<{ name: string; unit: string; unitPrice: number; vatRate: number; sku: string }>> {
    return this.#write<{ rows?: Array<{ name: string; unit: string; unitPrice: number; vatRate: number; sku: string }> }>(
      "POST",
      "/ai/extract-price-list",
      { dataUrl },
    ).then((response) => response.rows ?? []);
  }

  /** Creates a price-list item. */
  createProduct(draft: ProductDraft): Promise<BillingProduct> {
    return this.#write<{ product: BillingProduct }>("POST", "/billing/products", draft).then(
      (r) => r.product,
    );
  }

  /** Edits a price-list item. Never rewrites a document already raised — a
   *  line copies name, unit, price and rate at the moment it is picked. */
  updateProduct(id: string, draft: ProductDraft): Promise<BillingProduct> {
    return this.#write<{ product: BillingProduct }>(
      "PATCH",
      `/billing/products/${encodeURIComponent(id)}`,
      draft,
    ).then((r) => r.product);
  }

  /** Archives or restores a price-list item. */
  setProductArchived(id: string, archived: boolean): Promise<BillingProduct> {
    return this.#write<{ product: BillingProduct }>(
      "POST",
      `/billing/products/${encodeURIComponent(id)}/archive`,
      { archived },
    ).then((r) => r.product);
  }

  /** The tenant's invoices, newest first, each with its totals, settlement and
   *  computed `overdue` flag but without its lines. `status` narrows the list on
   *  the server — an unknown value is a `422` there, never a silently wider
   *  list. */
  invoices(status?: InvoiceStatus): Promise<BillingInvoiceSummary[]> {
    const query = status === undefined ? "" : `?status=${encodeURIComponent(status)}`;
    return this.#read<{ invoices?: BillingInvoiceSummary[] }>(`/billing/invoices${query}`).then(
      (r) => r.invoices ?? [],
    );
  }

  /** What is still owed past its date: issued, not settled, and judged against
   *  the **server's** date — so a browser with a wrong clock can neither clear
   *  nor invent a late invoice. A separate call rather than a `status` value,
   *  because overdue is a view over the issued ones, not a fifth state. */
  overdueInvoices(): Promise<BillingInvoiceSummary[]> {
    return this.#read<{ invoices?: BillingInvoiceSummary[] }>("/billing/invoices?overdue=1").then(
      (r) => r.invoices ?? [],
    );
  }

  /** One whole document — header, lines, totals — with the credit notes raised
   *  against it (empty for an uncredited document, and for a credit note) and
   *  the payments received against it, newest first. */
  invoice(id: string): Promise<{
    invoice: BillingInvoice;
    creditNotes: BillingInvoiceSummary[];
    payments: BillingPayment[];
  }> {
    return this.#read<{
      invoice: BillingInvoice;
      creditNotes?: BillingInvoiceSummary[];
      payments?: BillingPayment[];
    }>(`/billing/invoices/${encodeURIComponent(id)}`).then((r) => ({
      invoice: r.invoice,
      creditNotes: r.creditNotes ?? [],
      payments: r.payments ?? [],
    }));
  }

  /** Raises a **draft**. Only `customerId` is required; an absent currency or
   *  payment term takes the customer's own and is then snapshotted. */
  createInvoice(draft: InvoiceDraft): Promise<BillingInvoice> {
    return this.#write<{ invoice: BillingInvoice }>("POST", "/billing/invoices", draft).then(
      (r) => r.invoice,
    );
  }

  /** Saves a draft: the stated header fields merge onto the stored ones, and
   *  `lines` replaces the whole set in the order sent. Answers the stored
   *  document, whose `totals` are the only totals the editor ever shows. */
  updateInvoice(id: string, draft: InvoiceDraft): Promise<BillingInvoice> {
    return this.#write<{ invoice: BillingInvoice }>(
      "PATCH",
      `/billing/invoices/${encodeURIComponent(id)}`,
      draft,
    ).then((r) => r.invoice);
  }

  /** Discards a **draft**. The only document that is ever removed: it never
   *  consumed a number, so abandoning it leaves no hole in the series. */
  async deleteInvoice(id: string): Promise<void> {
    await this.#json<unknown>(
      await this.#send(`/billing/invoices/${encodeURIComponent(id)}`, { method: "DELETE" }),
    );
  }

  /**
   * Issues a draft: the server assigns the next number of the tenant's series,
   * stamps the issue and due dates from its own clock, and freezes the
   * document. Not idempotent — a second call answers `409`, so a client that
   * retried after a timeout reads what happened instead of spending a second
   * number on one invoice.
   */
  issueInvoice(id: string): Promise<BillingInvoice> {
    return this.#act<{ invoice: BillingInvoice }>(
      `/billing/invoices/${encodeURIComponent(id)}/issue`,
    ).then((r) => r.invoice);
  }

  /** Cancels an issued invoice. It keeps its number and stays readable — a
   *  document the customer already holds is corrected with a credit note. */
  voidInvoice(id: string): Promise<BillingInvoice> {
    return this.#act<{ invoice: BillingInvoice }>(
      `/billing/invoices/${encodeURIComponent(id)}/void`,
    ).then((r) => r.invoice);
  }

  /** Raises a **draft** credit note mirroring every line of an issued or paid
   *  invoice. Editing it down before issuing is how a partial credit is made. */
  createCreditNote(id: string): Promise<BillingInvoice> {
    return this.#act<{ invoice: BillingInvoice }>(
      `/billing/invoices/${encodeURIComponent(id)}/credit-note`,
    ).then((r) => r.invoice);
  }

  /** The payments recorded against one document, newest first, and what they
   *  add up to. The settlement is the server's — the browser never sums money. */
  payments(id: string): Promise<{ payments: BillingPayment[]; settlement: DocumentSettlement }> {
    return this.#read<{ payments?: BillingPayment[]; settlement: DocumentSettlement }>(
      `/billing/invoices/${encodeURIComponent(id)}/payments`,
    ).then((r) => ({ payments: r.payments ?? [], settlement: r.settlement }));
  }

  /**
   * Records money received against an issued invoice, and answers the document
   * **after** the ledger changed — so a caller that has just posted the last
   * instalment learns in the same response that the invoice is now `paid`.
   *
   * The store refuses a document that cannot carry money (a draft, a void one,
   * a credit note) with a `409`, and an amount that is not positive, or a date
   * in the future, with a `422`.
   */
  recordPayment(
    id: string,
    draft: PaymentDraft,
  ): Promise<{ payment: BillingPayment; invoice: BillingInvoice }> {
    return this.#write<{ payment: BillingPayment; invoice: BillingInvoice }>(
      "POST",
      `/billing/invoices/${encodeURIComponent(id)}/payments`,
      draft,
    );
  }

  /** Removes a payment recorded wrongly — the correction path, and the only
   *  one: a payment is a fact that happened, so it is re-entered rather than
   *  edited. Answers the document the removal left behind. */
  async deletePayment(id: string, paymentId: string): Promise<BillingInvoice> {
    const answer = await this.#json<{ invoice: BillingInvoice }>(
      await this.#send(
        `/billing/invoices/${encodeURIComponent(id)}/payments/${encodeURIComponent(paymentId)}`,
        { method: "DELETE" },
      ),
    );
    return answer.invoice;
  }

  /**
   * Writes a payment reminder for one invoice into the sender's own Drafts and
   * answers what it says (B1.26).
   *
   * **Nothing is sent**: the letter lands where the user reads it, changes a
   * word and sends it themselves, which is the rule the whole module follows
   * for mail. Calling it twice writes two drafts and changes no billing record,
   * so the button needs no confirmation and no idempotency key.
   *
   * The request states nothing about the money: who it goes to, what is left
   * and how late it is are read off the stored document by the server. `note`
   * is the one thing a person may add, and the server bounds it (500 characters
   * → `422`).
   */
  remindInvoice(id: string, note?: string): Promise<ReminderDraft> {
    return this.#write<{ draft: ReminderDraft }>(
      "POST",
      `/billing/invoices/${encodeURIComponent(id)}/reminder${langQuery()}`,
      note === undefined || note.trim() === "" ? {} : { note },
    ).then((r) => r.draft);
  }

  /** The tenant's quotes, newest first, each with its totals and computed
   *  `expired` flag but without its lines. An unknown `status` is a `422`. */
  quotes(status?: QuoteStatus): Promise<BillingQuoteSummary[]> {
    const query = status === undefined ? "" : `?status=${encodeURIComponent(status)}`;
    return this.#read<{ quotes?: BillingQuoteSummary[] }>(`/billing/quotes${query}`).then(
      (r) => r.quotes ?? [],
    );
  }

  /** One whole offer — header, lines, totals — and the id of the draft invoice
   *  its acceptance raised (`null` for every offer that was not accepted). */
  quote(id: string): Promise<{ quote: BillingQuote; invoiceId: string | null }> {
    return this.#read<{ quote: BillingQuote; invoiceId?: string | null }>(
      `/billing/quotes/${encodeURIComponent(id)}`,
    ).then((r) => ({ quote: r.quote, invoiceId: r.invoiceId ?? null }));
  }

  /** The studio's design of an offer, whole — the server keeps it as saved —
   *  or `null` for an offer nobody designed yet. */
  quoteDesign(id: string): Promise<{ design: unknown; updatedAt: string | null }> {
    return this.#read<{ design?: unknown; updatedAt?: string | null }>(
      `/billing/quotes/${encodeURIComponent(id)}/design`,
    ).then((r) => ({ design: r.design ?? null, updatedAt: r.updatedAt ?? null }));
  }

  /** Replaces the design of a **draft** offer. A sent offer answers `409`:
   *  the paper the customer holds is frozen, its presentation with it. */
  saveQuoteDesign(id: string, design: unknown): Promise<void> {
    return this.#write<{ design: unknown }>(
      "PUT",
      `/billing/quotes/${encodeURIComponent(id)}/design`,
      design,
    ).then(() => undefined);
  }

  /** The offer as the PDF file the customer receives, with the name the
   *  server gave it. Fetched rather than linked, like `documentHtml`. */
  async quotePdf(id: string): Promise<{ blob: Blob; fileName: string }> {
    const res = await this.#send(
      `/billing/quotes/${encodeURIComponent(id)}/pdf${langQuery()}`,
      { method: "GET" },
    );
    if (!res.ok) throw new BillingError(res.status, await problemDetail(res));
    const disposition = res.headers.get("content-disposition") ?? "";
    const named = /filename="([^"]+)"/.exec(disposition);
    return { blob: await res.blob(), fileName: named?.[1] ?? "quote.pdf" };
  }

  /** Raises a **draft** offer. Only `customerId` is required; the currency and
   *  the validity fall back to the customer's and the server's defaults. */
  createQuote(draft: QuoteDraft): Promise<BillingQuote> {
    return this.#write<{ quote: BillingQuote }>("POST", "/billing/quotes", draft).then(
      (r) => r.quote,
    );
  }

  /** Saves a draft offer, with the same merge rules as an invoice. */
  updateQuote(id: string, draft: QuoteDraft): Promise<BillingQuote> {
    return this.#write<{ quote: BillingQuote }>(
      "PATCH",
      `/billing/quotes/${encodeURIComponent(id)}`,
      draft,
    ).then((r) => r.quote);
  }

  /** Discards a **draft** offer, which was never made to anybody. */
  async deleteQuote(id: string): Promise<void> {
    await this.#json<unknown>(
      await this.#send(`/billing/quotes/${encodeURIComponent(id)}`, { method: "DELETE" }),
    );
  }

  /** Records that the offer was made: the server assigns its number, stamps
   *  the send date and the day it stands until, and freezes the content. It
   *  sends no email — the mail draft is B1.18's. */
  sendQuote(id: string): Promise<BillingQuote> {
    return this.#act<{ quote: BillingQuote }>(
      `/billing/quotes/${encodeURIComponent(id)}/send`,
    ).then((r) => r.quote);
  }

  /** The customer took the offer. One server transaction closes the quote and
   *  raises the document it becomes, so both come back.
   *
   *  **Which document depends on the offer's lines.** One naming a price-list
   *  item is for goods and becomes a draft *sales order* — goods are reserved,
   *  picked and delivered before anybody is billed — and `invoice` is then null.
   *  An offer of services becomes the draft invoice it always did, with
   *  `salesOrder` null. Exactly one of the two is ever present. */
  acceptQuote(id: string): Promise<QuoteAcceptance> {
    return this.#act<QuoteAcceptance>(`/billing/quotes/${encodeURIComponent(id)}/accept`);
  }

  /** The offer was turned down, or withdrawn. Terminal: a change of mind is a
   *  new quote. */
  declineQuote(id: string): Promise<BillingQuote> {
    return this.#act<{ quote: BillingQuote }>(
      `/billing/quotes/${encodeURIComponent(id)}/decline`,
    ).then((r) => r.quote);
  }

  /** The offer lapsed without an answer and somebody stopped chasing it. */
  expireQuote(id: string): Promise<BillingQuote> {
    return this.#act<{ quote: BillingQuote }>(
      `/billing/quotes/${encodeURIComponent(id)}/expire`,
    ).then((r) => r.quote);
  }

  /** The tenant's recurring arrangements, newest first, each with what ONE
   *  occurrence is worth, how many drafts it has raised, and whether a run now
   *  would raise something (all computed by the server). */
  schedules(): Promise<BillingScheduleSummary[]> {
    return this.#read<{ schedules?: BillingScheduleSummary[] }>("/billing/schedules").then(
      (r) => r.schedules ?? [],
    );
  }

  /** One arrangement with its template lines, and the drafts it has raised —
   *  one read, because that is one question. */
  schedule(id: string): Promise<{ schedule: BillingSchedule; invoices: BillingInvoiceSummary[] }> {
    return this.#read<{ schedule: BillingSchedule; invoices?: BillingInvoiceSummary[] }>(
      `/billing/schedules/${encodeURIComponent(id)}`,
    ).then((r) => ({ schedule: r.schedule, invoices: r.invoices ?? [] }));
  }

  /** Sets up an arrangement. `customerId`, `cadence`, `startDate` and a
   *  non-empty `lines` are required — the server refuses anything less, since
   *  an arrangement with nothing to bill is not an arrangement. */
  createSchedule(draft: ScheduleDraft): Promise<BillingSchedule> {
    return this.#write<{ schedule: BillingSchedule }>("POST", "/billing/schedules", draft).then(
      (r) => r.schedule,
    );
  }

  /** Edits what stays editable: name, cadence, end date, reference, note and
   *  the template. The customer, currency, terms and start date are not among
   *  them, and the server ignores them here. */
  updateSchedule(id: string, draft: ScheduleDraft): Promise<BillingSchedule> {
    return this.#write<{ schedule: BillingSchedule }>(
      "PATCH",
      `/billing/schedules/${encodeURIComponent(id)}`,
      draft,
    ).then((r) => r.schedule);
  }

  /** Pauses or resumes an arrangement. A paused one keeps every date and
   *  resumes where it left off — the months it missed were still months the
   *  customer was under contract for. */
  setScheduleActive(id: string, active: boolean): Promise<BillingSchedule> {
    return this.#act<{ schedule: BillingSchedule }>(
      `/billing/schedules/${encodeURIComponent(id)}/${active ? "resume" : "pause"}`,
    ).then((r) => r.schedule);
  }

  /** Deletes an arrangement that has never raised anything. One that has is a
   *  `409` telling you to pause it instead — its documents point back at it. */
  async deleteSchedule(id: string): Promise<void> {
    await this.#json<unknown>(
      await this.#send(`/billing/schedules/${encodeURIComponent(id)}`, { method: "DELETE" }),
    );
  }

  /** Raises the drafts every due arrangement has come due for, as of the
   *  SERVER's date, and answers the documents themselves. Safe to click twice:
   *  an occurrence is billed once. The same call the hourly background run
   *  makes — this is only "do not make me wait for it". */
  runSchedules(): Promise<BillingInvoice[]> {
    return this.#act<{ invoices?: BillingInvoice[] }>("/billing/schedules/run").then(
      (r) => r.invoices ?? [],
    );
  }

  /** Who the tenant invoices as. Never a `404` — an identity that has never
   *  been saved reads as the blanks with `stated: false`. */
  settings(): Promise<BillingSettings> {
    return this.#read<{ settings: BillingSettings }>("/billing/settings").then((r) => r.settings);
  }

  /** Saves the issuer identity. Absent fields keep their stored value; `null`
   *  clears a nullable one. The legal name is required by the server, so the
   *  first save must carry one. */
  saveSettings(draft: SettingsDraft): Promise<BillingSettings> {
    return this.#write<{ settings: BillingSettings }>("PATCH", "/billing/settings", draft).then(
      (r) => r.settings,
    );
  }

  /**
   * What was billed at each VAT rate between two days, both included — the
   * figures a VAT return is copied from (B1.20).
   *
   * Both days are required by the server (`422` otherwise): a summary that
   * quietly defaulted to a period would put a figure under a heading nobody
   * asked for. Every figure comes back in integer cents, per currency; the
   * browser sums nothing.
   */
  vatReport(from: string, to: string): Promise<VatReport> {
    return this.#read<{ report: VatReport }>(`/billing/reports/vat?${period(from, to)}`).then(
      (r) => r.report,
    );
  }

  /**
   * The same summary as a CSV file, rendered by the server from the same read
   * — so what an accountant opens and what the tenant is looking at cannot
   * disagree about a cent.
   *
   * Fetched rather than linked, like the print view: the route is
   * authenticated, and a plain `<a href>` would download a `401`.
   */
  vatReportCsv(from: string, to: string): Promise<string> {
    return this.#text(`/billing/reports/vat.csv?${period(from, to)}`);
  }

  /**
   * The exchange rates this tenant has, newest publication day first (B1.21).
   *
   * `currency` narrows to one code; `from`/`to` to a period. Everything is
   * optional — a rate list with no period is "everything I have", and the server
   * caps the answer.
   */
  fxRates(filter: { currency?: string; from?: string; to?: string } = {}): Promise<FxRate[]> {
    const query = new URLSearchParams();
    if (filter.currency !== undefined && filter.currency !== "") {
      query.set("currency", filter.currency);
    }
    if (filter.from !== undefined && filter.from !== "") query.set("from", filter.from);
    if (filter.to !== undefined && filter.to !== "") query.set("to", filter.to);
    const suffix = query.toString() === "" ? "" : `?${query.toString()}`;
    return this.#read<{ rates: FxRate[] }>(`/billing/fx/rates${suffix}`).then((r) => r.rates);
  }

  /**
   * Writes one rate by hand. Sending the same currency and day again replaces
   * it — that is how a typo, or a published correction, is fixed.
   *
   * Documents already issued are unaffected: each carries its own frozen rate,
   * so correcting the table can never restate a document a customer holds.
   */
  saveFxRate(draft: FxRateDraft): Promise<FxRate> {
    return this.#write<{ rate: FxRate }>("PUT", "/billing/fx/rates", draft).then((r) => r.rate);
  }

  /**
   * Imports a published euro reference-rate file — the ECB's `eurofxref` CSV, or
   * any file in that shape.
   *
   * All or nothing: a file with one bad cell changes nothing and comes back as a
   * `422` naming the row and the column. The text is sent as `text/csv`, because
   * what a user has is a file, not JSON.
   */
  async importFxRates(csv: string): Promise<FxImport> {
    const res = await this.#send("/billing/fx/rates/import", {
      method: "POST",
      headers: { "content-type": "text/csv; charset=utf-8" },
      body: csv,
    });
    return (await this.#json<{ import: FxImport }>(res)).import;
  }

  /**
   * The printable document as one self-contained HTML page — the page the
   * customer receives, rendered by the server (`docs/design/billing.md`).
   *
   * Deliberately fetched rather than linked: the route is authenticated, and a
   * plain `<a href>` would open a tab with no bearer token. The caller hands
   * the HTML to `printSheet`.
   */
  documentHtml(kind: "invoices" | "quotes", id: string): Promise<string> {
    return this.#text(`/billing/${kind}/${encodeURIComponent(id)}/print${langQuery()}`);
  }

  async #read<T>(path: string): Promise<T> {
    return this.#json<T>(await this.#send(path, { method: "GET" }));
  }

  /** A `GET` whose body is not JSON. A failure still carries the server's
   *  `Problem` detail, which is JSON — the same error shape as everywhere. */
  async #text(path: string): Promise<string> {
    const res = await this.#send(path, { method: "GET" });
    if (!res.ok) throw new BillingError(res.status, await problemDetail(res));
    return res.text();
  }

  /** A lifecycle transition: a `POST` that carries no input at all. What the
   *  document becomes is the route, never a field a stale form could send. */
  async #act<T>(path: string): Promise<T> {
    return this.#json<T>(await this.#send(path, { method: "POST" }));
  }

  async #write<T>(method: string, path: string, body: unknown): Promise<T> {
    return this.#json<T>(
      await this.#send(path, {
        method,
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      }),
    );
  }

  async #send(path: string, init: RequestInit): Promise<Response> {
    try {
      return await this.#fetch(`${API_BASE}/api${path}`, init);
    } catch {
      // A dropped connection is not a status code; give it one the UI can
      // treat like any other failure rather than an unhandled rejection.
      throw new BillingError(0, null);
    }
  }

  async #json<T>(res: Response): Promise<T> {
    if (!res.ok) throw new BillingError(res.status, await problemDetail(res));
    return (await res.json()) as T;
  }
}

/** The billing client bound to the current session. Memoized per auth context,
 *  so a re-render never re-creates it and effects keyed on it do not loop. */
export function useBillingApi(): BillingApi {
  const { authorizedFetch } = useAuth();
  return useMemo(() => new BillingApi(authorizedFetch), [authorizedFetch]);
}
