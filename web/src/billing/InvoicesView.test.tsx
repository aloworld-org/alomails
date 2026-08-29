import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useParams } from "react-router-dom";
import { afterEach, describe, expect, test, vi } from "vitest";

import { strings } from "../i18n";
import type { BillingCustomer, BillingInvoiceSummary } from "./types";

const { billingApi } = vi.hoisted(() => ({
  billingApi: {
    invoices: vi.fn(),
    overdueInvoices: vi.fn(),
    customers: vi.fn(),
    remindInvoice: vi.fn(),
  },
}));

vi.mock("./api", async (loadOriginal) => {
  const original = await loadOriginal<typeof import("./api")>();
  return { ...original, useBillingApi: () => billingApi };
});

import { InvoicesView } from "./InvoicesView";

const customer: BillingCustomer = {
  id: "customer-75",
  name: "Copper Digital 75",
  addressLine1: "Fictional Avenue 75",
  addressLine2: "",
  postalCode: "1000",
  city: "Brussels",
  country: "BE",
  vatId: "BE0010000075",
  email: "accounts@copper-digital-75.example",
  paymentTermsDays: 30,
  currency: "EUR",
  contactId: null,
  archived: false,
  archivedAt: null,
  createdBy: "user-1",
  createdAt: "2026-08-01T10:00:00Z",
  updatedAt: "2026-08-28T10:00:00Z",
};

const draft: BillingInvoiceSummary = {
  id: "recurring-draft-75",
  customerId: customer.id,
  status: "draft",
  currency: "EUR",
  number: null,
  issueDate: null,
  dueDate: null,
  paymentTermsDays: 30,
  overdue: false,
  creditNote: false,
  creditsInvoiceId: null,
  quoteId: null,
  scheduleId: "schedule-75",
  scheduleDueDate: "2026-08-29",
  reference: "",
  note: "",
  createdBy: "user-1",
  createdAt: "2026-08-29T10:00:00Z",
  updatedAt: "2026-08-29T10:00:00Z",
  totals: {
    netCents: 96000,
    vatCents: 20160,
    grossCents: 116160,
    vatByRate: [{ rateBp: 2100, netCents: 96000, vatCents: 20160 }],
  },
  fx: null,
  settlement: {
    grossCents: 116160,
    paidCents: 0,
    outstandingCents: 116160,
    state: "unpaid",
  },
};

function OpenedInvoice() {
  const { invoiceId } = useParams();
  return <p data-testid="opened-invoice">{invoiceId}</p>;
}

function renderView() {
  billingApi.invoices.mockResolvedValue([draft]);
  billingApi.customers.mockResolvedValue([customer]);
  return render(
    <MemoryRouter initialEntries={["/billing/invoices"]}>
      <Routes>
        <Route path="/billing/invoices" element={<InvoicesView />} />
        <Route
          path="/billing/invoices/new"
          element={<p data-testid="new-invoice" />}
        />
        <Route
          path="/billing/invoices/:invoiceId"
          element={<OpenedInvoice />}
        />
      </Routes>
    </MemoryRouter>,
  );
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("InvoicesView", () => {
  test("names an unnumbered recurring draft and opens that exact invoice from its row", async () => {
    renderView();

    const row = await screen.findByRole("link", {
      name: `${strings.billingDraftInvoice}: ${customer.name}`,
    });
    expect(screen.getByText(strings.billingDraftInvoice)).toBeTruthy();

    fireEvent.click(row);

    expect((await screen.findByTestId("opened-invoice")).textContent).toBe(
      draft.id,
    );
    expect(screen.queryByTestId("new-invoice")).toBeNull();
  });

  test("opens the exact invoice with the keyboard", async () => {
    renderView();

    const row = await screen.findByRole("link", {
      name: `${strings.billingDraftInvoice}: ${customer.name}`,
    });
    fireEvent.keyDown(row, { key: "Enter" });

    expect((await screen.findByTestId("opened-invoice")).textContent).toBe(
      draft.id,
    );
  });
});
