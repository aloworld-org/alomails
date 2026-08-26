// What the quote screens promise, proven against a recorded network: that an
// offer is edited exactly like an invoice draft, that each of its four
// transitions says what it will do before it does it, and that accepting one
// really lands on the draft invoice the server raised from it.
//
// Only the network is fake. The real router, the real module routes, the real
// client, the real shared editor shell and the real line model all run — the
// point of the item is that a quote and an invoice are the same screen, and a
// test against stubs could not tell.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { MemoryRouter, Route, Routes, useParams } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import { BillingModule } from "./BillingModule";
import type {
  BillingCustomer,
  BillingInvoice,
  BillingProduct,
  BillingQuote,
} from "./types";

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

/** Queues one answer for the next request whose URL contains `urlPart`. */
function reply(urlPart: string, method: string, body: unknown, status = 200) {
  replies.push({
    match: (url, m) => url.includes(urlPart) && m === method,
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

const PRODUCT: BillingProduct = {
  id: "p-1",
  name: "Consulting",
  unit: "hour",
  unitPriceCents: 12500,
  vatRateBp: 2100,
  // The catalog half (B5.02/B5.03): a service, so nothing about it is stocked.
  sku: "",
  barcode: "",
  stocked: false,
  purchasePriceCents: 0,
  photoNodeId: null,
  defaultSupplierId: null,
  archived: false,
  archivedAt: null,
  createdBy: "u-1",
  createdAt: "2026-08-06T10:00:00Z",
  updatedAt: "2026-08-06T10:00:00Z",
};

const DRAFT: BillingQuote = {
  id: "quo-1",
  customerId: "c-1",
  status: "draft",
  currency: "EUR",
  number: null,
  sentDate: null,
  validUntil: null,
  validDays: 30,
  decidedDate: null,
  expired: false,
  reference: "RFQ-77",
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
};

/** An offer that was made, and whose validity has since run out — the state
 *  the store still allows to be accepted. */
const SENT: BillingQuote = {
  ...DRAFT,
  id: "quo-2",
  status: "sent",
  number: "QUO-2026-00004",
  sentDate: "2026-07-01",
  validUntil: "2026-07-31",
  expired: true,
};

/** The draft invoice accepting `SENT` produces: the same lines, the same
 *  totals, no number of its own yet. */
const FROM_QUOTE: BillingInvoice = {
  id: "inv-9",
  customerId: "c-1",
  status: "draft",
  currency: "EUR",
  number: null,
  issueDate: null,
  dueDate: null,
  paymentTermsDays: 14,
  overdue: false,
  creditNote: false,
  creditsInvoiceId: null,
  quoteId: "quo-2",
  scheduleId: null,
  scheduleDueDate: null,
  reference: "RFQ-77",
  note: "",
  createdBy: "u-1",
  createdAt: "2026-08-07T10:00:00Z",
  updatedAt: "2026-08-07T10:00:00Z",
  lines: DRAFT.lines,
  totals: DRAFT.totals,
  fx: null,
  settlement: {
    grossCents: DRAFT.totals.grossCents,
    paidCents: 0,
    outstandingCents: DRAFT.totals.grossCents,
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
      : url.includes("/billing/customers")
        ? { customers: [CUSTOMER] }
        : url.includes("/billing/products")
          ? { products: [PRODUCT] }
          : { quotes: [] };
  return { match: () => true, status: 200, body };
}

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

/** The module as it is really mounted: at `/billing/*`, routing itself. */
/** Stands in for the order screen, which belongs to another module. What this
 *  file has to prove is only that an accepted offer for goods leaves for the
 *  right order id. */
function OrderScreenStub() {
  const { id } = useParams<{ id: string }>();
  return <p>order screen for {id}</p>;
}

function ui(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <DialogProvider>
        <Routes>
          <Route path="/billing/*" element={<BillingModule />} />
          <Route
            path="/inventory/sales-orders/:id"
            element={<OrderScreenStub />}
          />
        </Routes>
      </DialogProvider>
    </MemoryRouter>,
  );
}

function lastWrite(): Call | undefined {
  return calls.filter((c) => c.method !== "GET").at(-1);
}

/** Answers the confirmation dialog, whose confirm button carries the same
 *  label as the action that opened it. */
function press(label: string) {
  const buttons = screen.getAllByRole("button", { name: label });
  fireEvent.click(buttons[buttons.length - 1] as HTMLElement);
}

beforeEach(() => {
  calls.length = 0;
  replies = [];
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("the quote list", () => {
  test("shows the server's number, customer and total, and marks what has lapsed", async () => {
    reply("/billing/quotes", "GET", { quotes: [SENT] });
    ui("/billing/quotes");

    expect(await screen.findByText("QUO-2026-00004")).toBeTruthy();
    const table = within(screen.getByRole("table"));
    expect(table.getByText("Acme GmbH")).toBeTruthy();
    // €226.88 is the server's gross; nothing here adds up the lines.
    expect(table.getByText("€226.88")).toBeTruthy();
    expect(table.getByText("Jul 1, 2026")).toBeTruthy();
    expect(table.getByText("Jul 31, 2026")).toBeTruthy();
    expect(table.getByText(strings.billingQuoteStatusSent)).toBeTruthy();
    // The computed flag, worded apart from the "Expired" status.
    expect(table.getByText(strings.billingQuoteLapsed)).toBeTruthy();
    expect(table.queryByText(strings.billingQuoteStatusExpired)).toBeNull();
  });

  test("the status filter asks the server, rather than filtering a loaded page", async () => {
    reply("/billing/quotes", "GET", { quotes: [DRAFT] });
    ui("/billing/quotes");
    await screen.findByText(strings.billingStatusDraft);

    reply("/billing/quotes", "GET", { quotes: [SENT] });
    fireEvent.change(
      screen.getByLabelText(strings.billingFilterStatus, { exact: false }),
      {
        target: { value: "sent" },
      },
    );

    await waitFor(() =>
      expect(
        calls.some((c) => c.url.includes("/billing/quotes?status=sent")),
      ).toBe(true),
    );
  });
});

describe("the quote draft editor", () => {
  test("a draft is raised for the chosen customer, and nothing else is sent", async () => {
    ui("/billing/quotes/new");

    fireEvent.click(
      await screen.findByRole("combobox", {
        name: strings.billingFieldCustomer,
      }),
    );
    fireEvent.click(screen.getByRole("option", { name: CUSTOMER.name }));
    reply("/billing/quotes", "POST", {
      quote: { ...DRAFT, lines: [], reference: "" },
    });
    fireEvent.click(
      screen.getByRole("button", {
        name: strings.billingQuoteContinueToEditor,
      }),
    );

    await waitFor(() => expect(lastWrite()).toBeTruthy());
    const write = lastWrite();
    expect(write?.method).toBe("POST");
    expect(write?.url).toContain("/billing/quotes");
    // The blanks stay absent, so the customer's currency and the server's own
    // default validity still apply.
    expect(write?.body).toEqual({ customerId: "c-1" });
  });

  test("a template copies live price-list items into the persisted draft", async () => {
    ui("/billing/quotes/new");

    fireEvent.click(
      await screen.findByRole("button", {
        name: new RegExp(strings.billingQuoteTemplateServices),
      }),
    );
    expect(screen.getByText("Consulting")).toBeTruthy();
    fireEvent.click(
      screen.getByRole("combobox", { name: strings.billingFieldCustomer }),
    );
    fireEvent.click(screen.getByRole("option", { name: CUSTOMER.name }));
    reply("/billing/quotes", "POST", { quote: DRAFT });
    fireEvent.click(
      screen.getByRole("button", {
        name: strings.billingQuoteContinueToEditor,
      }),
    );

    await waitFor(() => expect(lastWrite()?.method).toBe("POST"));
    expect(lastWrite()?.body).toEqual({
      customerId: "c-1",
      lines: [
        {
          description: "Consulting",
          unit: "hour",
          qtyMilli: 1000,
          unitPriceCents: 12500,
          vatRateBp: 2100,
          productId: "p-1",
        },
      ],
    });
  });

  test("a blank quote can add any active price-list item before it is created", async () => {
    ui("/billing/quotes/new");

    fireEvent.click(
      await screen.findByRole("button", {
        name: strings.billingQuoteAddFromPriceList,
      }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: new RegExp(PRODUCT.name) }),
    );

    expect(
      screen.getByRole("button", {
        name: strings.billingQuoteRemoveIncludedItem(PRODUCT.name),
      }),
    ).toBeTruthy();
    expect(screen.getByText(strings.billingQuoteIncludedItems(1))).toBeTruthy();
  });

  test("a typed quantity is saved as milli-units and the totals shown are the server's", async () => {
    reply("/billing/quotes/quo-1", "GET", { quote: DRAFT, invoiceId: null });
    ui("/billing/quotes/quo-1");
    expect(await screen.findByText("€226.88")).toBeTruthy();

    reply("/billing/quotes/quo-1", "PATCH", {
      // Deliberately not what the lines multiply out to.
      quote: {
        ...DRAFT,
        lines: [
          { ...(DRAFT.lines[0] as object), qtyMilli: 2000, netCents: 25000 },
        ],
        totals: {
          netCents: 25000,
          vatCents: 5250,
          grossCents: 99999,
          vatByRate: [{ rateBp: 2100, netCents: 25000, vatCents: 5250 }],
        },
      },
    });
    fireEvent.change(screen.getByLabelText(strings.billingColQty), {
      target: { value: "2" },
    });

    await waitFor(() => expect(lastWrite()).toBeTruthy(), { timeout: 3000 });
    const write = lastWrite();
    expect(write?.method).toBe("PATCH");
    // Only the line set: the customer is not restated, for the same reason as
    // on an invoice — an offer whose customer was archived afterwards must
    // still be editable.
    expect(write?.body).toEqual({
      lines: [
        {
          description: "Consulting",
          unit: "hour",
          qtyMilli: 2000,
          unitPriceCents: 12500,
          vatRateBp: 2100,
        },
      ],
    });
    expect(await screen.findByText("€999.99")).toBeTruthy();
  });

  test("a refusal is shown in the server's own words and nothing is lost", async () => {
    reply("/billing/quotes/quo-1", "GET", { quote: DRAFT, invoiceId: null });
    ui("/billing/quotes/quo-1");
    await screen.findByText("€226.88");

    reply(
      "/billing/quotes/quo-1",
      "PATCH",
      { detail: "line 1: description must not be empty" },
      422,
    );
    fireEvent.change(screen.getByLabelText(strings.billingColQty), {
      target: { value: "3" },
    });

    expect(await screen.findByRole("alert")).toHaveProperty(
      "textContent",
      "line 1: description must not be empty",
    );
    expect(
      (screen.getByLabelText(strings.billingColQty) as HTMLInputElement).value,
    ).toBe("3");
  });
});

describe("the offer's transitions", () => {
  test("editing a finalized quote creates an editable revision with the same lines", async () => {
    reply("/billing/quotes/quo-2", "GET", { quote: SENT, invoiceId: null });
    ui("/billing/quotes/quo-2");
    await screen.findByText(strings.billingQuoteSentNotice);

    reply("/billing/quotes", "POST", {
      quote: { ...DRAFT, id: "quo-revision", lines: SENT.lines },
    });
    reply("/billing/quotes/quo-revision", "GET", {
      quote: { ...DRAFT, id: "quo-revision", lines: SENT.lines },
      invoiceId: null,
    });
    fireEvent.click(screen.getByRole("button", { name: "Edit quote" }));

    await waitFor(() => expect(lastWrite()?.url).toContain("/billing/quotes"));
    expect(lastWrite()?.method).toBe("POST");
    expect(lastWrite()?.body).toMatchObject({
      customerId: SENT.customerId,
      lines: SENT.lines.map((line) => ({
        description: line.description,
        unit: line.unit,
        qtyMilli: line.qtyMilli,
        unitPriceCents: line.unitPriceCents,
        vatRateBp: line.vatRateBp,
      })),
    });
  });

  test("each state offers only its own, and a closed offer offers none", async () => {
    reply("/billing/quotes/quo-1", "GET", { quote: DRAFT, invoiceId: null });
    ui("/billing/quotes/quo-1");
    await screen.findByText("€226.88");
    expect(
      screen.getByRole("button", { name: strings.billingSendQuote }),
    ).toBeTruthy();
    for (const label of [
      strings.billingAcceptQuote,
      strings.billingDeclineQuote,
    ]) {
      expect(screen.queryByRole("button", { name: label })).toBeNull();
    }
    cleanup();

    reply("/billing/quotes/quo-2", "GET", { quote: SENT, invoiceId: null });
    ui("/billing/quotes/quo-2");
    const finalizedNote = await screen.findByText(
      strings.billingQuoteSentNotice,
    );
    const history = finalizedNote.closest("section");
    expect(history).not.toBeNull();
    expect(
      within(history as HTMLElement).getByText(strings.auditHistoryTitle),
    ).toBeTruthy();
    for (const label of [
      strings.billingAcceptQuote,
      strings.billingDeclineQuote,
      strings.billingExpireQuote,
    ]) {
      expect(screen.getByRole("button", { name: label })).toBeTruthy();
    }
    // A lapsed offer can still be accepted: the store refuses on state, never
    // on a date, so this screen must not lock the door either.
    expect(screen.getByText(strings.billingQuoteLapsed)).toBeTruthy();
    expect(
      screen.queryByRole("button", { name: strings.billingSendQuote }),
    ).toBeNull();
    cleanup();

    reply("/billing/quotes/quo-3", "GET", {
      quote: {
        ...SENT,
        id: "quo-3",
        status: "declined",
        decidedDate: "2026-08-01",
      },
      invoiceId: null,
    });
    ui("/billing/quotes/quo-3");
    expect(
      await screen.findByText(strings.billingQuoteClosedNotice),
    ).toBeTruthy();
    for (const label of [
      strings.billingSendQuote,
      strings.billingAcceptQuote,
      strings.billingDeclineQuote,
      strings.billingExpireQuote,
    ]) {
      expect(screen.queryByRole("button", { name: label })).toBeNull();
    }
  });

  test("sending says it takes a number and freezes the prices, then does exactly that", async () => {
    reply("/billing/quotes/quo-1", "GET", { quote: DRAFT, invoiceId: null });
    ui("/billing/quotes/quo-1");
    await screen.findByText("€226.88");

    fireEvent.click(
      screen.getByRole("button", { name: strings.billingSendQuote }),
    );
    expect(
      await screen.findByText(strings.billingSendQuoteConfirm),
    ).toBeTruthy();

    // Backing out writes nothing: no number is spent by looking.
    fireEvent.click(screen.getByRole("button", { name: strings.dialogCancel }));
    await waitFor(() =>
      expect(screen.queryByText(strings.billingSendQuoteConfirm)).toBeNull(),
    );
    expect(lastWrite()).toBeUndefined();

    fireEvent.click(
      screen.getByRole("button", { name: strings.billingSendQuote }),
    );
    await screen.findByText(strings.billingSendQuoteConfirm);
    reply("/billing/quotes/quo-1/send", "POST", {
      quote: { ...SENT, id: "quo-1", expired: false },
    });
    press(strings.billingSendQuote);

    await waitFor(() => expect(lastWrite()).toBeTruthy());
    expect(lastWrite()?.url).toContain("/billing/quotes/quo-1/send");
    // A transition carries no input at all.
    expect(lastWrite()?.body).toBeUndefined();

    expect(await screen.findByText("QUO-2026-00004")).toBeTruthy();
    expect(screen.getByText(strings.billingQuoteSentNotice)).toBeTruthy();
    expect(screen.queryByLabelText(strings.billingColQty)).toBeNull();
  });

  test("accepting closes the offer and lands on the draft invoice it raised", async () => {
    reply("/billing/quotes/quo-2", "GET", { quote: SENT, invoiceId: null });
    ui("/billing/quotes/quo-2");
    await screen.findByText(strings.billingQuoteSentNotice);

    fireEvent.click(
      screen.getByRole("button", { name: strings.billingAcceptQuote }),
    );
    expect(
      await screen.findByText(strings.billingAcceptQuoteConfirm),
    ).toBeTruthy();
    // Deliberately no `salesOrder` key at all, not `salesOrder: null` — this
    // is the shape a server that predates the order routing answers with, and
    // the screen must still land on the invoice. Do not "complete" this
    // fixture: reading an absent field as a raised order is the exact bug it
    // exists to catch.
    reply("/billing/quotes/quo-2/accept", "POST", {
      quote: { ...SENT, status: "accepted", decidedDate: "2026-08-07" },
      invoice: FROM_QUOTE,
    });
    reply("/billing/invoices/inv-9", "GET", {
      invoice: FROM_QUOTE,
      creditNotes: [],
    });
    press(strings.billingAcceptQuote);

    await waitFor(() =>
      expect(lastWrite()?.url).toContain("/billing/quotes/quo-2/accept"),
    );
    // The invoice the server made is where the work now is: an editable draft
    // worth exactly what the offer was, that knows where it came from.
    expect(await screen.findByText(strings.billingDraftInvoice)).toBeTruthy();
    expect(screen.getByText("€226.88")).toBeTruthy();
    expect(screen.getByLabelText(strings.billingColQty)).toBeTruthy();
    expect(calls.some((c) => c.url.includes("/billing/invoices/inv-9"))).toBe(
      true,
    );

    // And the arc closes: the invoice names the offer it came from, and that
    // link really goes back to it rather than to a sibling invoice id.
    reply("/billing/quotes/quo-2", "GET", {
      quote: { ...SENT, status: "accepted", decidedDate: "2026-08-07" },
      invoiceId: "inv-9",
    });
    fireEvent.click(
      screen.getByRole("button", { name: strings.billingFromQuote }),
    );
    expect(await screen.findByText("QUO-2026-00004")).toBeTruthy();
    expect(screen.getByText(strings.billingQuoteStatusAccepted)).toBeTruthy();
  });

  test("an accepted offer for goods lands on the order it raised, not an invoice", async () => {
    reply("/billing/quotes/quo-2", "GET", { quote: SENT, invoiceId: null });
    ui("/billing/quotes/quo-2");
    await screen.findByText(strings.billingQuoteSentNotice);

    fireEvent.click(
      screen.getByRole("button", { name: strings.billingAcceptQuote }),
    );
    expect(
      await screen.findByText(strings.billingAcceptQuoteConfirm),
    ).toBeTruthy();
    // An offer whose lines name stocked items is for goods: the server raises
    // a sales order and no invoice, because nothing is billed until something
    // is delivered.
    reply("/billing/quotes/quo-2/accept", "POST", {
      quote: { ...SENT, status: "accepted", decidedDate: "2026-08-07" },
      invoice: null,
      salesOrder: { id: "so-7" },
    });
    press(strings.billingAcceptQuote);

    await waitFor(() =>
      expect(lastWrite()?.url).toContain("/billing/quotes/quo-2/accept"),
    );
    expect(await screen.findByText("order screen for so-7")).toBeTruthy();
    // And it did not go looking for an invoice that was never raised.
    expect(calls.some((c) => c.url.includes("/billing/invoices/"))).toBe(false);
  });

  test("declining closes the offer for good, in the server's own answer", async () => {
    reply("/billing/quotes/quo-2", "GET", { quote: SENT, invoiceId: null });
    ui("/billing/quotes/quo-2");
    await screen.findByText(strings.billingQuoteSentNotice);

    fireEvent.click(
      screen.getByRole("button", { name: strings.billingDeclineQuote }),
    );
    expect(
      await screen.findByText(strings.billingDeclineQuoteConfirm),
    ).toBeTruthy();
    reply("/billing/quotes/quo-2/decline", "POST", {
      quote: { ...SENT, status: "declined", decidedDate: "2026-08-07" },
    });
    press(strings.billingDeclineQuote);

    expect(
      await screen.findByText(strings.billingQuoteClosedNotice),
    ).toBeTruthy();
    expect(screen.getByText(strings.billingQuoteStatusDeclined)).toBeTruthy();
    // The number stays, and so does the document.
    expect(screen.getByText("QUO-2026-00004")).toBeTruthy();
  });

  test("an accepted offer names the invoice it became", async () => {
    reply("/billing/quotes/quo-2", "GET", {
      quote: { ...SENT, status: "accepted", decidedDate: "2026-08-07" },
      invoiceId: "inv-9",
    });
    ui("/billing/quotes/quo-2");
    await screen.findByText(strings.billingQuoteClosedNotice);

    reply("/billing/invoices/inv-9", "GET", {
      invoice: FROM_QUOTE,
      creditNotes: [],
    });
    fireEvent.click(
      screen.getByRole("button", { name: strings.billingQuoteInvoice }),
    );

    expect(await screen.findByText(strings.billingDraftInvoice)).toBeTruthy();
    expect(calls.some((c) => c.url.includes("/billing/invoices/inv-9"))).toBe(
      true,
    );
  });

  test("a refused transition is reported in the server's own words", async () => {
    reply("/billing/quotes/quo-1", "GET", {
      quote: { ...DRAFT, lines: [] },
      invoiceId: null,
    });
    ui("/billing/quotes/quo-1");
    await screen.findByRole("button", { name: strings.billingSendQuote });

    fireEvent.click(
      screen.getByRole("button", { name: strings.billingSendQuote }),
    );
    await screen.findByText(strings.billingSendQuoteConfirm);
    reply(
      "/billing/quotes/quo-1/send",
      "POST",
      { detail: "a quote with no lines cannot be sent" },
      422,
    );
    press(strings.billingSendQuote);

    expect(await screen.findByRole("alert")).toHaveProperty(
      "textContent",
      "a quote with no lines cannot be sent",
    );
    // Still a draft, still editable.
    expect(
      screen.getByRole("button", { name: strings.billingAddLine }),
    ).toBeTruthy();
  });
});
