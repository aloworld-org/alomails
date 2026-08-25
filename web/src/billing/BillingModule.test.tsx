// The wiring the type checker cannot see: that a list really renders what the
// API answered, that a form really sends integer cents and basis points, and
// that a refusal from the server is shown to the user instead of swallowed.
//
// The auth layer is stubbed down to one recording `fetch`, so the REAL client,
// the real views and the real money parsing all run — only the network is
// fake.
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import { CustomersView } from "./CustomersView";
import { ProductsView } from "./ProductsView";
import { BILLING_DEFAULT_PATH } from "./BillingModule";
import type { BillingCustomer, BillingProduct } from "./types";

interface Call {
  url: string;
  method: string;
  body: unknown;
}

const calls: Call[] = [];
/** What the next request answers, in order; falls back to an empty list. */
let answers: { status: number; body: unknown }[] = [];

const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
  calls.push({
    url,
    method: init?.method ?? "GET",
    body: typeof init?.body === "string" ? JSON.parse(init.body) : undefined,
  });
  const next = answers.shift() ?? {
    status: 200,
    body: { customers: [], products: [] },
  };
  return new Response(JSON.stringify(next.body), {
    status: next.status,
    headers: { "content-type": "application/json" },
  });
});

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

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

function ui(children: React.ReactNode) {
  return render(<DialogProvider>{children}</DialogProvider>);
}

beforeEach(() => {
  calls.length = 0;
  answers = [];
  fakeFetch.mockClear();
});

// The suite runs without vitest globals, so React Testing Library cannot
// register its own auto-cleanup: without this, one test's dialog is still in
// the document while the next one queries for a field of the same name.
afterEach(cleanup);

test("Billing opens on invoices instead of setup data", () => {
  expect(BILLING_DEFAULT_PATH).toBe("/billing/invoices");
});

describe("the customer list", () => {
  test("shows what the API answered, archived rows included on request", async () => {
    answers = [{ status: 200, body: { customers: [CUSTOMER] } }];
    ui(<CustomersView />);

    expect(await screen.findByText("Acme GmbH")).toBeTruthy();
    expect(screen.getByText("DE811907980")).toBeTruthy();
    expect(screen.getByText(strings.billingTermsDays(14))).toBeTruthy();
    expect(calls[0]?.url).toContain("/billing/customers");
    expect(calls[0]?.url).not.toContain("includeArchived");

    answers = [
      { status: 200, body: { customers: [{ ...CUSTOMER, archived: true }] } },
    ];
    fireEvent.click(screen.getByLabelText(strings.billingShowArchived));
    await waitFor(() => expect(calls[1]?.url).toContain("includeArchived=1"));
    expect(await screen.findByText(strings.billingArchived)).toBeTruthy();
  });

  test("a create sends only what was filled in, and blanks stay absent", async () => {
    answers = [{ status: 200, body: { customers: [] } }];
    ui(<CustomersView />);

    // The toolbar and the empty state both offer the action; either will do.
    fireEvent.click(
      (
        await screen.findAllByRole("button", {
          name: strings.billingNewCustomer,
        })
      )[0]!,
    );
    fireEvent.change(
      screen.getByLabelText(strings.billingFieldName, { exact: false }),
      {
        target: { value: "  Acme GmbH  " },
      },
    );
    fireEvent.change(
      screen.getByLabelText(strings.billingFieldCountry, { exact: false }),
      {
        target: { value: "de" },
      },
    );
    answers = [
      { status: 200, body: { customer: CUSTOMER } },
      { status: 200, body: { customers: [CUSTOMER] } },
    ];
    fireEvent.click(
      screen.getByRole("button", { name: strings.billingCreate }),
    );

    await waitFor(() => expect(calls.length).toBeGreaterThan(1));
    const create = calls[1];
    expect(create?.method).toBe("POST");
    // Trimmed, and the untouched fields are absent rather than sent as "" —
    // the server's own defaults (EUR, 30-day terms) must still apply.
    expect(create?.body).toEqual({ name: "Acme GmbH", country: "de" });
  });

  test("clearing the VAT id sends an explicit null", async () => {
    answers = [{ status: 200, body: { customers: [CUSTOMER] } }];
    ui(<CustomersView />);

    fireEvent.click(await screen.findByRole("button", { name: "Acme GmbH" }));
    fireEvent.change(
      screen.getByLabelText(strings.billingFieldVatId, { exact: false }),
      {
        target: { value: "  " },
      },
    );
    answers = [
      { status: 200, body: { customer: { ...CUSTOMER, vatId: null } } },
    ];
    fireEvent.click(screen.getByRole("button", { name: strings.billingSave }));

    await waitFor(() => expect(calls.length).toBeGreaterThan(1));
    expect(calls[1]?.method).toBe("PATCH");
    expect(calls[1]?.url).toContain("/billing/customers/c-1");
    expect(calls[1]?.body).toEqual({ vatId: null });
  });

  test("a refusal is shown in the server's own words, with the form intact", async () => {
    answers = [{ status: 200, body: { customers: [] } }];
    ui(<CustomersView />);

    // The toolbar and the empty state both offer the action; either will do.
    fireEvent.click(
      (
        await screen.findAllByRole("button", {
          name: strings.billingNewCustomer,
        })
      )[0]!,
    );
    const name = screen.getByLabelText(strings.billingFieldName, {
      exact: false,
    });
    fireEvent.change(name, { target: { value: "Acme" } });
    fireEvent.change(
      screen.getByLabelText(strings.billingFieldVatId, { exact: false }),
      {
        target: { value: "DE811907981" },
      },
    );
    answers = [
      {
        status: 422,
        body: {
          detail:
            "the check digit of this DE VAT id does not match; check for a typo",
        },
      },
    ];
    fireEvent.click(
      screen.getByRole("button", { name: strings.billingCreate }),
    );

    expect(await screen.findByRole("alert")).toHaveProperty(
      "textContent",
      "the check digit of this DE VAT id does not match; check for a typo",
    );
    // Still open, still holding what was typed: the point of the message is
    // that it can be fixed in place.
    expect((name as HTMLInputElement).value).toBe("Acme");
  });
});

describe("the price list", () => {
  test("renders money the server sent without recomputing it", async () => {
    answers = [{ status: 200, body: { products: [PRODUCT] } }];
    ui(<ProductsView />);

    expect(await screen.findByText("Consulting")).toBeTruthy();
    expect(screen.getByText("125.00")).toBeTruthy();
    expect(screen.getByText("21%")).toBeTruthy();
  });

  test("a typed price becomes integer cents and a typed rate becomes basis points", async () => {
    answers = [{ status: 200, body: { products: [] } }];
    ui(<ProductsView />);

    fireEvent.click(
      (
        await screen.findAllByRole("button", {
          name: strings.billingNewProduct,
        })
      )[0]!,
    );
    fireEvent.change(
      screen.getByLabelText(strings.billingFieldName, { exact: false }),
      {
        target: { value: "Consulting" },
      },
    );
    fireEvent.change(
      screen.getByLabelText(strings.billingFieldUnitPrice, { exact: false }),
      {
        target: { value: "1 234,56" },
      },
    );
    fireEvent.change(
      screen.getByLabelText(strings.billingFieldVatRate, { exact: false }),
      {
        target: { value: "5,5" },
      },
    );
    answers = [
      { status: 200, body: { product: PRODUCT } },
      { status: 200, body: { products: [PRODUCT] } },
    ];
    fireEvent.click(
      screen.getByRole("button", { name: strings.billingCreate }),
    );

    await waitFor(() => expect(calls.length).toBeGreaterThan(1));
    expect(calls[1]?.body).toEqual({
      name: "Consulting",
      unitPriceCents: 123456,
      vatRateBp: 550,
    });
  });

  test("a price that is not a number is never sent", async () => {
    answers = [{ status: 200, body: { products: [] } }];
    ui(<ProductsView />);

    fireEvent.click(
      (
        await screen.findAllByRole("button", {
          name: strings.billingNewProduct,
        })
      )[0]!,
    );
    fireEvent.change(
      screen.getByLabelText(strings.billingFieldName, { exact: false }),
      {
        target: { value: "Consulting" },
      },
    );
    fireEvent.change(
      screen.getByLabelText(strings.billingFieldUnitPrice, { exact: false }),
      {
        target: { value: "twelve fifty" },
      },
    );

    expect(await screen.findByText(strings.billingNotAnAmount)).toBeTruthy();
    const submit = screen.getByRole("button", { name: strings.billingCreate });
    expect((submit as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(submit);
    expect(calls.length).toBe(1); // the initial list, and nothing else
  });
});
