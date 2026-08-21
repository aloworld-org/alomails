// What the invoice screens promise, proven against a recorded network: that
// every figure on them is one the server sent, that a typed quantity reaches
// the API as milli-units and a typed price as integer cents, that a row which
// is not yet a line stops the save instead of being dropped from it, and that
// a document carrying a number offers no edits at all.
//
// Only the network is fake. The real router, the real module routes, the real
// client and the real line model all run — a test that stubbed those would be
// testing a drawing of the editor.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import { BillingModule } from "./BillingModule";
import type { BillingCustomer, BillingInvoice, BillingProduct } from "./types";

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
/** Answers queued for specific requests; the first match wins and is spent. */
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

const DRAFT: BillingInvoice = {
  id: "inv-1",
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
  // A draft carries no exchange rate: the rate is frozen when it is issued.
  fx: null,
  // Nothing received: the settlement the server sends for a document nobody
  // has paid anything against.
  settlement: {
    grossCents: 22688,
    paidCents: 0,
    outstandingCents: 22688,
    state: "unpaid",
  },
};

const ISSUED: BillingInvoice = {
  ...DRAFT,
  id: "inv-2",
  status: "issued",
  number: "INV-2026-00007",
  issueDate: "2026-07-01",
  dueDate: "2026-07-15",
  overdue: true,
  // €100.00 of €226.88 received: still issued, still overdue, and still owed
  // for the rest — which is what "partly paid is not a status" means.
  settlement: {
    grossCents: 22688,
    paidCents: 10000,
    outstandingCents: 12688,
    state: "partiallyPaid",
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
  const answer = index === -1 ? fallback(url, method) : (replies.splice(index, 1)[0] as Reply);
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
        ? { payments: [], settlement: DRAFT.settlement }
        : url.includes("/billing/customers")
          ? { customers: [CUSTOMER] }
          : url.includes("/billing/products")
            ? { products: [PRODUCT] }
            : { invoices: [] };
  return { match: () => true, status: 200, body };
}

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

/** The module as it is really mounted: at `/billing/*`, routing itself. */
function ui(path: string, state?: unknown) {
  return render(
    <MemoryRouter
      initialEntries={[state === undefined ? path : { pathname: path, state }]}
    >
      <DialogProvider>
        <Routes>
          <Route path="/billing/*" element={<BillingModule />} />
          <Route
            path="/projects/:projectId/overview"
            element={<p>Project overview destination</p>}
          />
        </Routes>
      </DialogProvider>
    </MemoryRouter>,
  );
}

/** The last write the client made, if it made one. */
function lastWrite(): Call | undefined {
  return calls.filter((c) => c.method !== "GET").at(-1);
}

/** Answers the confirmation dialog. Its confirm button carries the same label
 *  as the action that opened it, and it is rendered after the page, so the
 *  last one is the dialog's. */
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

describe("the invoice list", () => {
  test("shows the server's number, customer and total, and marks what is late", async () => {
    reply("/billing/invoices", "GET", { invoices: [ISSUED] });
    ui("/billing/invoices");

    expect(await screen.findByText("INV-2026-00007")).toBeTruthy();
    const row = within(screen.getByRole("table"));
    expect(row.getByText("Acme GmbH")).toBeTruthy();
    // €226.88 is the server's gross and €126.88 what is still owed after the
    // payments recorded against it; nothing here adds up or subtracts anything.
    expect(row.getByText("€226.88")).toBeTruthy();
    expect(row.getByText("€126.88")).toBeTruthy();
    // The chips, not the filter's options, which carry the same words.
    expect(row.getByText(strings.billingStatusIssued)).toBeTruthy();
    expect(row.getByText(strings.billingStatusOverdue)).toBeTruthy();
    expect(row.getByText("Jul 1, 2026")).toBeTruthy();
  });

  test("the status filter asks the server, rather than filtering a loaded page", async () => {
    reply("/billing/invoices", "GET", { invoices: [DRAFT] });
    ui("/billing/invoices");
    await screen.findByText(strings.billingStatusDraft);

    reply("/billing/invoices", "GET", { invoices: [ISSUED] });
    fireEvent.change(screen.getByLabelText(strings.billingFilterStatus, { exact: false }), {
      target: { value: "issued" },
    });

    await waitFor(() =>
      expect(calls.some((c) => c.url.includes("/billing/invoices?status=issued"))).toBe(true),
    );
  });
});

describe("raising a draft", () => {
  test("a draft is raised for the chosen customer, and nothing else is sent", async () => {
    ui("/billing/invoices/new");

    const picker = await screen.findByLabelText(strings.billingFieldCustomer);
    fireEvent.change(picker, { target: { value: "c-1" } });
    reply("/billing/invoices", "POST", { invoice: { ...DRAFT, lines: [], reference: "" } });
    fireEvent.click(screen.getByRole("button", { name: strings.billingCreateDraft }));

    await waitFor(() => expect(lastWrite()).toBeTruthy());
    const write = lastWrite();
    expect(write?.method).toBe("POST");
    expect(write?.url).toContain("/billing/invoices");
    // No lines, no totals, no number — a draft is raised, then filled in —
    // and the blanks are absent, so the customer's own currency and payment
    // term still apply.
    expect(write?.body).toEqual({ customerId: "c-1" });
  });
});

describe("the draft editor", () => {
  test("returns a project-created draft to its originating project", async () => {
    reply("/billing/invoices/inv-1", "GET", { invoice: DRAFT, creditNotes: [] });
    ui("/billing/invoices/inv-1", {
      fromProject: { id: "project/website", name: "Website redesign" },
    });

    const back = await screen.findByRole("button", {
      name: strings.billingBackToProject("Website redesign"),
    });
    fireEvent.click(back);

    expect(await screen.findByText("Project overview destination")).toBeTruthy();
  });

  test("a typed quantity is saved as milli-units and the new totals are the server's", async () => {
    reply("/billing/invoices/inv-1", "GET", { invoice: DRAFT, creditNotes: [] });
    ui("/billing/invoices/inv-1");

    // What the API said this draft is worth, before anything is touched.
    expect(await screen.findByText("€226.88")).toBeTruthy();

    reply("/billing/invoices/inv-1", "PATCH", {
      // Deliberately not what the lines multiply out to: whatever the server
      // says a document is worth is what the screen must show.
      invoice: {
        ...DRAFT,
        lines: [{ ...DRAFT.lines[0]!, qtyMilli: 2000, netCents: 25000 }],
        totals: {
          netCents: 25000,
          vatCents: 5250,
          grossCents: 99999,
          vatByRate: [{ rateBp: 2100, netCents: 25000, vatCents: 5250 }],
        },
      },
    });
    fireEvent.change(screen.getByLabelText(strings.billingColQty), { target: { value: "2" } });

    await waitFor(() => expect(lastWrite()).toBeTruthy(), { timeout: 3000 });
    const write = lastWrite();
    expect(write?.method).toBe("PATCH");
    // Only the line set: nothing in the header changed. Restating the customer
    // would send the document back through the store's customer check on every
    // save, and a draft raised for a since-archived customer would then be
    // uneditable — proven on the wire, not guessed at.
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

  test("a changed header field is stated, and only that one", async () => {
    reply("/billing/invoices/inv-1", "GET", { invoice: DRAFT, creditNotes: [] });
    ui("/billing/invoices/inv-1");
    await screen.findByText("€226.88");

    reply("/billing/invoices/inv-1", "PATCH", { invoice: DRAFT });
    fireEvent.change(screen.getByPlaceholderText(strings.billingReferencePlaceholder), {
      target: { value: "PO-88" },
    });

    await waitFor(() => expect(lastWrite()).toBeTruthy(), { timeout: 3000 });
    const body = lastWrite()?.body as Record<string, unknown>;
    expect(body.reference).toBe("PO-88");
    expect(body.customerId).toBeUndefined();
    expect(body.note).toBeUndefined();
  });

  test("a price typed in any European notation reaches the API as integer cents", async () => {
    reply("/billing/invoices/inv-1", "GET", { invoice: DRAFT, creditNotes: [] });
    ui("/billing/invoices/inv-1");
    await screen.findByText("€226.88");

    reply("/billing/invoices/inv-1", "PATCH", { invoice: DRAFT });
    fireEvent.change(screen.getByLabelText(strings.billingColUnitPrice), {
      target: { value: "1 234,56" },
    });

    await waitFor(() => expect(lastWrite()).toBeTruthy(), { timeout: 3000 });
    expect(lastWrite()?.body).toHaveProperty("lines.0.unitPriceCents", 123456);
  });

  test("a row that is not a line yet stops the save instead of being dropped", async () => {
    reply("/billing/invoices/inv-1", "GET", { invoice: DRAFT, creditNotes: [] });
    ui("/billing/invoices/inv-1");
    await screen.findByText("€226.88");

    fireEvent.click(screen.getByRole("button", { name: strings.billingAddLine }));
    const prices = screen.getAllByLabelText(strings.billingColUnitPrice);
    fireEvent.change(prices[1]!, { target: { value: "50" } });

    expect(await screen.findByText(strings.billingLineNeedsDescription)).toBeTruthy();
    // Long enough that a debounce would have fired twice over.
    await new Promise((done) => setTimeout(done, 1500));
    expect(lastWrite()).toBeUndefined();
    expect(screen.getByText(strings.billingUnsaved)).toBeTruthy();
  });

  test("picking a price-list item copies its price and rate onto the line", async () => {
    reply("/billing/invoices/inv-1", "GET", { invoice: { ...DRAFT, lines: [] }, creditNotes: [] });
    ui("/billing/invoices/inv-1");
    await screen.findByRole("button", { name: strings.billingAddLine });

    fireEvent.click(screen.getByRole("button", { name: strings.billingAddLine }));
    reply("/billing/invoices/inv-1", "PATCH", { invoice: DRAFT });
    fireEvent.change(screen.getByLabelText(strings.billingPickProduct), {
      target: { value: "p-1" },
    });

    await waitFor(() => expect(lastWrite()).toBeTruthy(), { timeout: 3000 });
    expect(lastWrite()?.body).toHaveProperty("lines.0", {
      description: "Consulting",
      unit: "hour",
      // Nobody said how many, so the line bills one.
      qtyMilli: 1000,
      unitPriceCents: 12500,
      vatRateBp: 2100,
      // Which item was picked travels beside the copy of its figures. An
      // invoice has no column for it and its server drops it; on a *quote* it
      // is what decides whether accepting the offer raises an order or an
      // invoice, and the line grid is one grid shared by both.
      productId: "p-1",
    });
  });

  test("a refusal is shown in the server's own words and nothing is lost", async () => {
    reply("/billing/invoices/inv-1", "GET", { invoice: DRAFT, creditNotes: [] });
    ui("/billing/invoices/inv-1");
    await screen.findByText("€226.88");

    reply("/billing/invoices/inv-1", "PATCH", { detail: "a line needs a description" }, 422);
    fireEvent.change(screen.getByLabelText(strings.billingColQty), { target: { value: "3" } });

    expect(await screen.findByRole("alert")).toHaveProperty(
      "textContent",
      "a line needs a description",
    );
    expect((screen.getByLabelText(strings.billingColQty) as HTMLInputElement).value).toBe("3");
  });

  test("a document that carries a number offers no edits", async () => {
    reply("/billing/invoices/inv-2", "GET", { invoice: ISSUED, creditNotes: [] });
    ui("/billing/invoices/inv-2");

    expect(await screen.findByText(strings.billingFrozenNotice)).toBeTruthy();
    expect(screen.queryByLabelText(strings.billingColQty)).toBeNull();
    expect(screen.queryByRole("button", { name: strings.billingAddLine })).toBeNull();
    expect(screen.queryByRole("button", { name: strings.billingDeleteDraft })).toBeNull();
    // The stored line, formatted from the document rather than from a form.
    const table = screen.getByRole("table");
    expect(within(table).getByText("1.5")).toBeTruthy();
    expect(within(table).getByText("21%")).toBeTruthy();
  });
});

describe("the lifecycle actions", () => {
  test("issuing says what it will do, spends nothing until confirmed, and freezes", async () => {
    reply("/billing/invoices/inv-1", "GET", { invoice: DRAFT, creditNotes: [] });
    ui("/billing/invoices/inv-1");
    await screen.findByText("€226.88");

    fireEvent.click(screen.getByRole("button", { name: strings.billingIssue }));
    // The warning the item exists for: a number is spent and the document
    // freezes. It is the dialog's own words, not a paraphrase in the test.
    expect(await screen.findByText(strings.billingIssueConfirm)).toBeTruthy();

    // Backing out writes nothing at all: no number is consumed by looking.
    fireEvent.click(screen.getByRole("button", { name: strings.dialogCancel }));
    await waitFor(() => expect(screen.queryByText(strings.billingIssueConfirm)).toBeNull());
    expect(lastWrite()).toBeUndefined();

    fireEvent.click(screen.getByRole("button", { name: strings.billingIssue }));
    await screen.findByText(strings.billingIssueConfirm);
    reply("/billing/invoices/inv-1/issue", "POST", { invoice: { ...ISSUED, id: "inv-1" } });
    press(strings.billingIssue);

    await waitFor(() => expect(lastWrite()).toBeTruthy());
    const write = lastWrite();
    expect(write?.method).toBe("POST");
    expect(write?.url).toContain("/billing/invoices/inv-1/issue");
    // A transition carries no input: what the document becomes is the route,
    // never a field a stale form could have sent.
    expect(write?.body).toBeUndefined();

    // The document the server answered with, not an optimistic guess.
    expect(await screen.findByText("INV-2026-00007")).toBeTruthy();
    expect(screen.getByText(strings.billingFrozenNotice)).toBeTruthy();
    expect(screen.queryByLabelText(strings.billingColQty)).toBeNull();
    expect(screen.queryByRole("button", { name: strings.billingIssue })).toBeNull();
  });

  test("each state offers only its own transitions", async () => {
    reply("/billing/invoices/inv-1", "GET", { invoice: DRAFT, creditNotes: [] });
    ui("/billing/invoices/inv-1");
    await screen.findByText("€226.88");
    expect(screen.getByRole("button", { name: strings.billingIssue })).toBeTruthy();
    expect(screen.queryByRole("button", { name: strings.billingVoid })).toBeNull();
    expect(screen.queryByRole("button", { name: strings.billingCreditNoteAction })).toBeNull();
    cleanup();

    reply("/billing/invoices/inv-2", "GET", { invoice: ISSUED, creditNotes: [] });
    ui("/billing/invoices/inv-2");
    await screen.findByText(strings.billingFrozenNotice);
    expect(screen.getByRole("button", { name: strings.billingVoid })).toBeTruthy();
    expect(screen.getByRole("button", { name: strings.billingCreditNoteAction })).toBeTruthy();
    expect(screen.queryByRole("button", { name: strings.billingIssue })).toBeNull();
    cleanup();

    // A void document is finished: it offers nothing, rather than buttons the
    // store would refuse.
    reply("/billing/invoices/inv-3", "GET", {
      invoice: { ...ISSUED, id: "inv-3", status: "void", overdue: false },
      creditNotes: [],
    });
    ui("/billing/invoices/inv-3");
    expect(await screen.findByText(strings.billingVoidNotice)).toBeTruthy();
    for (const label of [strings.billingIssue, strings.billingVoid, strings.billingCreditNoteAction]) {
      expect(screen.queryByRole("button", { name: label })).toBeNull();
    }
  });

  test("a transition waits for the draft it would freeze to be saved", async () => {
    reply("/billing/invoices/inv-1", "GET", { invoice: DRAFT, creditNotes: [] });
    ui("/billing/invoices/inv-1");
    await screen.findByText("€226.88");

    // A row that cannot become a line keeps the draft unsent for good — and
    // issuing then would freeze a document that is not the one on screen.
    fireEvent.click(screen.getByRole("button", { name: strings.billingAddLine }));
    fireEvent.change(screen.getAllByLabelText(strings.billingColUnitPrice)[1] as HTMLElement, {
      target: { value: "50" },
    });

    expect(await screen.findByText(strings.billingActionsWaitForSave)).toBeTruthy();
    const issue = screen.getByRole("button", { name: strings.billingIssue });
    expect((issue as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(issue);
    // Long enough that a debounce would have fired twice over.
    await new Promise((done) => setTimeout(done, 1500));
    expect(screen.queryByText(strings.billingIssueConfirm)).toBeNull();
    expect(lastWrite()).toBeUndefined();
  });

  test("a credit note is raised as a draft, and the screen lands on it", async () => {
    reply("/billing/invoices/inv-2", "GET", { invoice: ISSUED, creditNotes: [] });
    ui("/billing/invoices/inv-2");
    await screen.findByText(strings.billingFrozenNotice);

    fireEvent.click(screen.getByRole("button", { name: strings.billingCreditNoteAction }));
    expect(await screen.findByText(strings.billingCreditNoteConfirm)).toBeTruthy();
    const credit = {
      ...DRAFT,
      id: "inv-9",
      creditNote: true,
      creditsInvoiceId: "inv-2",
      lines: [{ ...(DRAFT.lines[0] as object), qtyMilli: -1500, netCents: -18750 }],
      totals: {
        netCents: -18750,
        vatCents: -3938,
        grossCents: -22688,
        vatByRate: [{ rateBp: 2100, netCents: -18750, vatCents: -3938 }],
      },
    };
    reply("/billing/invoices/inv-2/credit-note", "POST", { invoice: credit });
    reply("/billing/invoices/inv-9", "GET", { invoice: credit, creditNotes: [] });
    press(strings.billingCreditNoteAction);

    // The mirror it made is what needs editing, so that is where we end up:
    // an editable draft, marked as a credit note, worth the negative of the
    // original — every figure of it the server's.
    expect(await screen.findByText("-€226.88")).toBeTruthy();
    expect(screen.getByText(strings.billingCreditNote)).toBeTruthy();
    expect(screen.getByLabelText(strings.billingColQty)).toBeTruthy();
    expect(screen.getByRole("button", { name: strings.billingCreditsInvoice })).toBeTruthy();
  });

  test("voiding keeps the number and says the document is worth nothing", async () => {
    reply("/billing/invoices/inv-2", "GET", { invoice: ISSUED, creditNotes: [] });
    ui("/billing/invoices/inv-2");
    await screen.findByText(strings.billingFrozenNotice);

    fireEvent.click(screen.getByRole("button", { name: strings.billingVoid }));
    expect(await screen.findByText(strings.billingVoidConfirm)).toBeTruthy();
    reply("/billing/invoices/inv-2/void", "POST", {
      invoice: { ...ISSUED, status: "void", overdue: false },
    });
    press(strings.billingVoid);

    expect(await screen.findByText(strings.billingVoidNotice)).toBeTruthy();
    // The number stays: a number that vanished is a hole in the series.
    expect(screen.getByText("INV-2026-00007")).toBeTruthy();
    expect(screen.getByText(strings.billingStatusVoid)).toBeTruthy();
  });

  test("a refused transition is reported in the server's own words", async () => {
    reply("/billing/invoices/inv-1", "GET", { invoice: DRAFT, creditNotes: [] });
    ui("/billing/invoices/inv-1");
    await screen.findByText("€226.88");

    fireEvent.click(screen.getByRole("button", { name: strings.billingIssue }));
    await screen.findByText(strings.billingIssueConfirm);
    reply(
      "/billing/invoices/inv-1/issue",
      "POST",
      { detail: "an invoice with no lines cannot be issued" },
      422,
    );
    press(strings.billingIssue);

    expect(await screen.findByRole("alert")).toHaveProperty(
      "textContent",
      "an invoice with no lines cannot be issued",
    );
    // Still a draft, still editable: a refusal changes nothing.
    expect(screen.getByLabelText(strings.billingColQty)).toBeTruthy();
  });
});
