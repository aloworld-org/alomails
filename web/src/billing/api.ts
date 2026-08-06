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
import { API_BASE } from "../platform/runtime";
import type {
  BillingCustomer,
  BillingInvoice,
  BillingInvoiceSummary,
  BillingProduct,
  BillingQuote,
  BillingQuoteSummary,
  CustomerDraft,
  InvoiceDraft,
  InvoiceStatus,
  ProductDraft,
  QuoteDraft,
  QuoteStatus,
} from "./types";

type AuthorizedFetch = (input: string, init?: RequestInit) => Promise<Response>;

/**
 * A failed billing request. `detail` is the server's own sentence when it sent
 * one — the store authors those messages to name the rule that was broken and
 * never to echo stored data, so they are safe to put in front of a user.
 * `status` lets a caller tell "you typed something impossible" (422) from
 * "that record is gone" (404) without parsing prose.
 */
export class BillingError extends Error {
  readonly status: number;
  readonly detail: string | null;

  constructor(status: number, detail: string | null) {
    super(detail ?? `billing request failed (${status})`);
    this.name = "BillingError";
    this.status = status;
    this.detail = detail;
  }
}

/**
 * What to show a user about a failed request: the server's own sentence when
 * it sent one, and `fallback` otherwise (a dropped connection, or a failure
 * whose reason is not the user's business). One helper, so every billing
 * screen reports a failure the same way.
 */
export function billingMessage(error: unknown, fallback: string): string {
  return error instanceof BillingError && error.detail !== null ? error.detail : fallback;
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

  /** The tenant's invoices, newest first, each with its totals and computed
   *  `overdue` flag but without its lines. `status` narrows the list on the
   *  server — an unknown value is a `422` there, never a silently wider list. */
  invoices(status?: InvoiceStatus): Promise<BillingInvoiceSummary[]> {
    const query = status === undefined ? "" : `?status=${encodeURIComponent(status)}`;
    return this.#read<{ invoices?: BillingInvoiceSummary[] }>(`/billing/invoices${query}`).then(
      (r) => r.invoices ?? [],
    );
  }

  /** One whole document — header, lines, totals — with the credit notes raised
   *  against it (empty for an uncredited document, and for a credit note). */
  invoice(id: string): Promise<{ invoice: BillingInvoice; creditNotes: BillingInvoiceSummary[] }> {
    return this.#read<{ invoice: BillingInvoice; creditNotes?: BillingInvoiceSummary[] }>(
      `/billing/invoices/${encodeURIComponent(id)}`,
    ).then((r) => ({ invoice: r.invoice, creditNotes: r.creditNotes ?? [] }));
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
   *  raises the draft invoice for it, so both documents come back. */
  acceptQuote(id: string): Promise<{ quote: BillingQuote; invoice: BillingInvoice }> {
    return this.#act<{ quote: BillingQuote; invoice: BillingInvoice }>(
      `/billing/quotes/${encodeURIComponent(id)}/accept`,
    );
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

  async #read<T>(path: string): Promise<T> {
    return this.#json<T>(await this.#send(path, { method: "GET" }));
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
      return await this.#fetch(`${API_BASE}${path}`, init);
    } catch {
      // A dropped connection is not a status code; give it one the UI can
      // treat like any other failure rather than an unhandled rejection.
      throw new BillingError(0, null);
    }
  }

  async #json<T>(res: Response): Promise<T> {
    if (!res.ok) {
      const problem = (await res.json().catch(() => ({}))) as { detail?: unknown };
      const detail = typeof problem.detail === "string" ? problem.detail : null;
      throw new BillingError(res.status, detail);
    }
    return (await res.json()) as T;
  }
}

/** The billing client bound to the current session. Memoized per auth context,
 *  so a re-render never re-creates it and effects keyed on it do not loop. */
export function useBillingApi(): BillingApi {
  const { authorizedFetch } = useAuth();
  return useMemo(() => new BillingApi(authorizedFetch), [authorizedFetch]);
}
