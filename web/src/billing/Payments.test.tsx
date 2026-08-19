// What the payments panel promises, proven against a recorded network: that a
// typed amount reaches the API as integer cents, that an empty date box is sent
// as no date at all (so the *server's* today decides), that the settlement
// figures on the screen are the ones the server sent rather than a browser sum,
// and that a document which cannot carry money is not offered the panel.
//
// Only the network is fake. The real router, the real module routes, the real
// client and the real money parser all run.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import { BillingModule } from "./BillingModule";
import type { BillingCustomer, BillingInvoice, BillingPayment } from "./types";

interface Call {
  url: string;
  method: string;
  body: unknown;
}

interface Reply {
  match: (url: string, method: string) => boolean;
  status: number;
  body: unknown;
}

const calls: Call[] = [];
let replies: Reply[] = [];

/** Queues one answer for the next request whose URL contains `urlPart`.
 *
 *  A document's URL is a prefix of its payment ledger's
 *  (`…/invoices/inv-2` and `…/invoices/inv-2/payments`), so a matcher written
 *  for the document deliberately does **not** swallow the ledger read the
 *  payments panel makes — otherwise the document's own answer would be spent on
 *  the wrong request and the screen would load nothing. */
function reply(urlPart: string, method: string, body: unknown, status = 200) {
  replies.push({
    match: (url, m) =>
      url.includes(urlPart) &&
      m === method &&
      (urlPart.includes("/payments") || !url.includes("/payments")),
    status,
    body,
  });
}

const CUSTOMER: BillingCustomer = {
  id: "c-1",
  name: "Acme GmbH",
  addressLine1: "Hauptstrasse 1",
  addressLine2: "",
  postalCode: "20095",
  city: "Hamburg",
  country: "DE",
  vatId: "DE811907980",
  email: "billing@acme.test",
  paymentTermsDays: 14,
  currency: "EUR",
  contactId: null,
  archived: false,
  archivedAt: null,
  createdBy: "u-1",
  createdAt: "2026-08-06T10:00:00Z",
  updatedAt: "2026-08-06T10:00:00Z",
};

/** An issued invoice worth €226.88 with €100.00 already received — the state
 *  the panel exists for. */
const PART_PAID: BillingInvoice = {
  id: "inv-2",
  customerId: "c-1",
  status: "issued",
  currency: "EUR",
  number: "INV-2026-00007",
  issueDate: "2026-07-01",
  dueDate: "2026-07-15",
  paymentTermsDays: 14,
  overdue: true,
  creditNote: false,
  creditsInvoiceId: null,
  quoteId: null,
  scheduleId: null,
  scheduleDueDate: null,
  reference: "PO-77",
  note: "",
  createdBy: "u-1",
  createdAt: "2026-08-06T10:00:00Z",
  updatedAt: "2026-08-06T10:00:00Z",
  lines: [
    {
      id: "l-1",
      description: "Consulting",
      unit: "hour",
      qtyMilli: 1500,
      unitPriceCents: 12500,
      vatRateBp: 2100,
      netCents: 18750,
    },
  ],
  totals: {
    netCents: 18750,
    vatCents: 3938,
    grossCents: 22688,
    vatByRate: [{ rateBp: 2100, netCents: 18750, vatCents: 3938 }],
  },
  fx: {
    baseCurrency: "EUR",
    rateMicro: 1_000_000,
    rate: "1.0",
    rateDate: "2026-08-06",
  },
  settlement: {
    grossCents: 22688,
    paidCents: 10000,
    outstandingCents: 12688,
    state: "partiallyPaid",
  },
};

const FIRST_PAYMENT: BillingPayment = {
  id: "pay-1",
  invoiceId: "inv-2",
  paidOn: "2026-07-10",
  amountCents: 10000,
  method: "SEPA direct debit",
  reference: "E2E-77",
  createdBy: "u-1",
  createdAt: "2026-07-10T09:00:00Z",
};

/** The same document once the rest of the money arrives: settled, and the
 *  status the server projected from its ledger. */
const SETTLED: BillingInvoice = {
  ...PART_PAID,
  status: "paid",
  overdue: false,
  settlement: {
    grossCents: 22688,
    paidCents: 22688,
    outstandingCents: 0,
    state: "paid",
  },
};

/** A credit note: money owed the other way, which never carries payments. */
const CREDIT_NOTE: BillingInvoice = {
  ...PART_PAID,
  id: "inv-3",
  number: "INV-2026-00008",
  creditNote: true,
  creditsInvoiceId: "inv-2",
  overdue: false,
  settlement: {
    grossCents: -22688,
    paidCents: 0,
    outstandingCents: -22688,
    state: "unpaid",
  },
};

const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
  const method = init?.method ?? "GET";
  calls.push({
    url,
    method,
    body: typeof init?.body === "string" ? JSON.parse(init.body) : undefined,
  });
  const index = replies.findIndex((r) => r.match(url, method));
  const answer =
    index === -1
      ? fallback(url, method)
      : (replies.splice(index, 1)[0] as Reply);
  return new Response(JSON.stringify(answer.body), {
    status: answer.status,
    headers: { "content-type": "application/json" },
  });
});

/** The lists a screen loads before anything interesting happens. */
function fallback(url: string, method: string): Reply {
  const body =
    method !== "GET"
      ? {}
      : url.includes("/payments")
        ? { payments: [], settlement: PART_PAID.settlement }
        : url.includes("/billing/customers")
          ? { customers: [CUSTOMER] }
          : url.includes("/billing/products")
            ? { products: [] }
            : { invoices: [] };
  return { match: () => true, status: 200, body };
}

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

function ui(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <DialogProvider>
        <Routes>
          <Route path="/billing/*" element={<BillingModule />} />
        </Routes>
      </DialogProvider>
    </MemoryRouter>,
  );
}

/** The last write the client made, if it made one. */
function lastWrite(): Call | undefined {
  return calls.filter((c) => c.method !== "GET").at(-1);
}

beforeEach(() => {
  calls.length = 0;
  replies = [];
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("the payment ledger of an invoice", () => {
  test("shows the server's received and outstanding figures, and the ledger rows", async () => {
    reply("/payments", "GET", {
      payments: [FIRST_PAYMENT],
      settlement: PART_PAID.settlement,
    });
    reply("/billing/invoices/inv-2", "GET", {
      invoice: PART_PAID,
      creditNotes: [],
      payments: [],
    });
    ui("/billing/invoices/inv-2");

    // The ledger is its own read, so the row arrives after the section does.
    expect(await screen.findByText("SEPA direct debit")).toBeTruthy();
    // By role rather than by text: the ledger is a `ds/Table` since D2.06b, and
    // a table carries its own name for a screen reader, so "Payments" is on the
    // page twice — as this heading and as the table's caption. The assertion
    // always meant the heading.
    expect(
      screen.getByRole("heading", { name: strings.billingPayments }),
    ).toBeTruthy();
    // €100.00 received of €226.88, so €126.88 is left — all three are the
    // server's numbers; nothing here subtracts anything.
    // Twice: once as the summary's "received", once as the row's own amount —
    // the same figure, because both came from the same server read.
    expect(screen.getAllByText("€100.00")).toHaveLength(2);
    expect(screen.getByText("€126.88")).toBeTruthy();
    expect(screen.getByText(strings.billingPaymentPartiallyPaid)).toBeTruthy();
    expect(screen.getByText("E2E-77")).toBeTruthy();
    expect(screen.getByText("Jul 10, 2026")).toBeTruthy();
  });

  test("a typed amount is sent as integer cents, and an empty date box sends no date", async () => {
    reply("/billing/invoices/inv-2", "GET", {
      invoice: PART_PAID,
      creditNotes: [],
      payments: [],
    });
    ui("/billing/invoices/inv-2");
    fireEvent.click(
      await screen.findByRole("button", { name: strings.billingRecordPayment }),
    );

    const amount = screen.getByLabelText(strings.billingFieldAmount("EUR"), {
      exact: false,
    });
    // The box starts at what is still outstanding — €126.88, the server's
    // figure — so settling in full is a confirmation, not a retyping.
    expect((amount as HTMLInputElement).value).toBe("126.88");
    fireEvent.change(amount, { target: { value: "126.88" } });
    fireEvent.change(
      screen.getByLabelText(strings.billingFieldMethod, { exact: false }),
      {
        target: { value: "bank transfer" },
      },
    );
    fireEvent.change(
      screen.getByLabelText(strings.billingFieldPaymentReference, {
        exact: false,
      }),
      {
        target: { value: "NL02RABO0123456789" },
      },
    );

    reply("/payments", "POST", {
      payment: { ...FIRST_PAYMENT, id: "pay-2", amountCents: 12688 },
      invoice: SETTLED,
    });
    const buttons = screen.getAllByRole("button", {
      name: strings.billingRecordPayment,
    });
    fireEvent.click(buttons[buttons.length - 1] as HTMLElement);

    await waitFor(() => expect(lastWrite()?.method).toBe("POST"));
    const write = lastWrite();
    expect(write?.url).toContain("/billing/invoices/inv-2/payments");
    // Integer cents, never a decimal — and no `paidOn` at all, so the server's
    // own date decides rather than the browser's.
    expect(write?.body).toEqual({
      amountCents: 12688,
      method: "bank transfer",
      reference: "NL02RABO0123456789",
    });

    // The document the server answered with is what the screen then shows: the
    // status it projected from the ledger, without a second read.
    expect(await screen.findByText(strings.billingPaymentPaid)).toBeTruthy();
  });

  test("removing a payment asks the server and adopts the document it answers", async () => {
    reply("/payments", "GET", {
      payments: [FIRST_PAYMENT],
      settlement: SETTLED.settlement,
    });
    reply("/billing/invoices/inv-2", "GET", {
      invoice: SETTLED,
      creditNotes: [],
      payments: [],
    });
    ui("/billing/invoices/inv-2");
    await screen.findByText("SEPA direct debit");

    reply("/payments/pay-1", "DELETE", { status: "ok", invoice: PART_PAID });
    fireEvent.click(
      screen.getByRole("button", { name: strings.billingRemovePayment }),
    );

    await waitFor(() => expect(lastWrite()?.method).toBe("DELETE"));
    expect(lastWrite()?.url).toContain(
      "/billing/invoices/inv-2/payments/pay-1",
    );
    // Back to owed: the money is not there, so the screen says so.
    expect(
      await screen.findByText(strings.billingPaymentPartiallyPaid),
    ).toBeTruthy();
  });

  test("the server's refusal is shown, never swallowed", async () => {
    reply("/billing/invoices/inv-2", "GET", {
      invoice: PART_PAID,
      creditNotes: [],
      payments: [],
    });
    ui("/billing/invoices/inv-2");
    fireEvent.click(
      await screen.findByRole("button", { name: strings.billingRecordPayment }),
    );

    reply(
      "/payments",
      "POST",
      { detail: "a payment cannot be dated in the future", status: 422 },
      422,
    );
    const buttons = screen.getAllByRole("button", {
      name: strings.billingRecordPayment,
    });
    fireEvent.click(buttons[buttons.length - 1] as HTMLElement);

    expect(
      await screen.findByText("a payment cannot be dated in the future"),
    ).toBeTruthy();
  });

  test("a credit note is never offered the panel — it is money owed the other way", async () => {
    reply("/billing/invoices/inv-3", "GET", {
      invoice: CREDIT_NOTE,
      creditNotes: [],
      payments: [],
    });
    ui("/billing/invoices/inv-3");

    expect(await screen.findByText("INV-2026-00008")).toBeTruthy();
    expect(screen.queryByText(strings.billingPayments)).toBeNull();
    expect(calls.some((c) => c.url.includes("/payments"))).toBe(false);
  });
});

describe("the overdue view", () => {
  test("is its own server read, not a filter over a loaded page", async () => {
    reply("/billing/invoices", "GET", { invoices: [PART_PAID] });
    ui("/billing/invoices");
    await screen.findByText("INV-2026-00007");
    // The list carries what is left, so the collections screen needs no second
    // call per row.
    expect(within(screen.getByRole("table")).getByText("€126.88")).toBeTruthy();

    reply("/billing/invoices?overdue=1", "GET", { invoices: [PART_PAID] });
    fireEvent.change(
      screen.getByLabelText(strings.billingFilterStatus, { exact: false }),
      {
        target: { value: "overdue" },
      },
    );

    await waitFor(() =>
      expect(
        calls.some((c) => c.url.includes("/billing/invoices?overdue=1")),
      ).toBe(true),
    );
    // Never as a status: `?status=overdue` would be a 422 from the server, and
    // asking for it at all would mean this client had invented a fifth state.
    expect(calls.some((c) => c.url.includes("status=overdue"))).toBe(false);
  });
});
