// What the dunning click promises, proven against a recorded network (B1.26):
// that one click on a late row asks the server for a reminder and nothing more,
// that what the user is then told is the server's own figures, that the wording
// says plainly the letter was not sent, and that a document nobody owes money
// on is not offered the click at all.
//
// Only the network is fake. The real router, the real module routes and the
// real client all run.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import { BillingModule } from "./BillingModule";
import type { BillingCustomer, BillingInvoiceSummary } from "./types";

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

/** Queues one answer for the next request whose URL contains `urlPart`. The
 *  list's URL is a prefix of the reminder's, so a matcher written for the list
 *  deliberately does not swallow the reminder's `POST`. */
function reply(urlPart: string, method: string, body: unknown, status = 200) {
  replies.push({
    match: (url, m) =>
      url.includes(urlPart) &&
      m === method &&
      (urlPart.includes("/reminder") || !url.includes("/reminder")),
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

/** An issued invoice worth €226.88 with €100.00 received and its date gone by:
 *  the state the reminder exists for. `overdue` is the server's flag. */
const LATE: BillingInvoiceSummary = {
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
  reference: "PO-77",
  note: "",
  createdBy: "u-1",
  createdAt: "2026-08-06T10:00:00Z",
  updatedAt: "2026-08-06T10:00:00Z",
  totals: {
    netCents: 18750,
    vatCents: 3938,
    grossCents: 22688,
    vatByRate: [{ rateBp: 2100, netCents: 18750, vatCents: 3938 }],
  },
  fx: { baseCurrency: "EUR", rateMicro: 1_000_000, rate: "1.0", rateDate: "2026-07-01" },
  settlement: {
    grossCents: 22688,
    paidCents: 10000,
    outstandingCents: 12688,
    state: "partiallyPaid",
  },
};

/** The same document once the money arrives: nobody owes anything. */
const SETTLED: BillingInvoiceSummary = {
  ...LATE,
  id: "inv-3",
  number: "INV-2026-00008",
  status: "paid",
  overdue: false,
  settlement: { grossCents: 22688, paidCents: 22688, outstandingCents: 0, state: "paid" },
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

/** The lists the screen loads before anything interesting happens. */
function fallback(url: string, method: string): Reply {
  const body =
    method !== "GET"
      ? {}
      : url.includes("/billing/customers")
        ? { customers: [CUSTOMER] }
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

/** The writes the client made, in order. */
function writes(): Call[] {
  return calls.filter((c) => c.method !== "GET");
}

/** The answer the route gives for `LATE`: the server's own figures. */
const DRAFTED = {
  draft: {
    id: "msg-9",
    invoice: "INV-2026-00007",
    to: "billing@acme.test",
    subject: "Reminder: Invoice INV-2026-00007 — Acme GmbH",
    daysOverdue: 23,
    outstandingCents: 12688,
  },
};

beforeEach(() => {
  calls.length = 0;
  replies = [];
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("chasing a late invoice from the list", () => {
  test("one click writes the reminder, and the answer is the server's figures", async () => {
    reply("/billing/invoices", "GET", { invoices: [LATE] });
    ui("/billing/invoices");
    await screen.findByText("INV-2026-00007");

    reply("/billing/invoices/inv-2/reminder", "POST", DRAFTED);
    fireEvent.click(screen.getByRole("button", { name: strings.billingRemind }));

    // Exactly one write, to the document's own reminder route. The request says
    // nothing about the money — who it goes to, what is left and how late it is
    // are the server's to read off the stored invoice.
    await waitFor(() => expect(writes().length).toBe(1));
    const write = writes()[0] as Call;
    expect(write.method).toBe("POST");
    expect(write.url).toContain("/billing/invoices/inv-2/reminder");
    expect(write.body).toEqual({});

    // €126.88 is the server's `outstandingCents`, not a browser subtraction of
    // the €100.00 received from the €226.88 total, and 23 days is its count.
    const notice = await screen.findByRole("status");
    expect(notice.textContent).toContain("INV-2026-00007");
    expect(notice.textContent).toContain("€126.88");
    expect(notice.textContent).toContain("23 days");
    // The one promise this screen must never break.
    expect(notice.textContent).toContain("Nothing has been sent");
    expect(notice.textContent).toContain("Drafts");
  });

  test("the list is not reloaded, because a reminder changes no invoice", async () => {
    reply("/billing/invoices", "GET", { invoices: [LATE] });
    ui("/billing/invoices");
    await screen.findByText("INV-2026-00007");
    const readsBefore = calls.filter((c) => c.method === "GET").length;

    reply("/billing/invoices/inv-2/reminder", "POST", DRAFTED);
    fireEvent.click(screen.getByRole("button", { name: strings.billingRemind }));
    await screen.findByRole("status");

    expect(calls.filter((c) => c.method === "GET").length).toBe(readsBefore);
    // …and the row is still there, still late, still showing the same figure.
    expect(within(screen.getByRole("table")).getByText("€126.88")).toBeTruthy();
  });

  test("a document nobody owes money on is not offered the click", async () => {
    reply("/billing/invoices", "GET", { invoices: [SETTLED] });
    ui("/billing/invoices");

    expect(await screen.findByText("INV-2026-00008")).toBeTruthy();
    expect(screen.queryByRole("button", { name: strings.billingRemind })).toBeNull();
  });

  test("a refusal is shown in the server's words, and claims no draft", async () => {
    reply("/billing/invoices", "GET", { invoices: [LATE] });
    ui("/billing/invoices");
    await screen.findByText("INV-2026-00007");

    reply(
      "/billing/invoices/inv-2/reminder",
      "POST",
      { detail: "this customer has no email address" },
      422,
    );
    fireEvent.click(screen.getByRole("button", { name: strings.billingRemind }));

    expect(await screen.findByText("this customer has no email address")).toBeTruthy();
    expect(screen.queryByRole("status")).toBeNull();
  });

  test("an empty overdue view says so, rather than reading as an empty ledger", async () => {
    reply("/billing/invoices", "GET", { invoices: [LATE] });
    ui("/billing/invoices");
    await screen.findByText("INV-2026-00007");

    reply("/billing/invoices?overdue=1", "GET", { invoices: [] });
    fireEvent.change(screen.getByLabelText(strings.billingFilterStatus, { exact: false }), {
      target: { value: "overdue" },
    });

    expect(await screen.findByText(strings.billingNothingOverdue)).toBeTruthy();
  });
});
