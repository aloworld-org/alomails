// What the two order screens promise, proven against a recorded network.
//
// Six claims, each of them something an order screen can silently get wrong:
//
//  - the list shows the **server's** state and its **server-computed** `late`,
//    and its filter asks the server rather than narrowing a page it already
//    has;
//  - picking a catalog item on a purchase order copies **what we pay**, not
//    what we charge, and the line carries the **catalog link** to the API —
//    which is the only reason a receipt can ever move real goods;
//  - **placing an order says all three things it does** before it happens, and
//    afterwards the document offers no field to edit and the letter waiting in
//    Drafts is named;
//  - the arrival sheet **opens on what is outstanding** and books exactly the
//    quantities typed over it;
//  - a refusal from the store is shown **in the server's own words**, with the
//    sheet still open and still holding what was typed;
//  - on a sales order, "to bill" is the **server's** invoiceable figure — the
//    browser never subtracts invoiced from delivered.
//
// Only the network is fake. The real router, the real module routes, the real
// clients, the real row model and the real i18n catalog all run.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import { InventoryModule } from "./InventoryModule";
import type { PurchaseOrder, SalesOrder } from "./types";

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
 *  An order's URL is a prefix of its receipts' (`…/po-1` and
 *  `…/po-1/receipts`), so a matcher written for the document deliberately does
 *  not swallow the consignment read the screen makes beside it. */
function reply(urlPart: string, method: string, body: unknown, status = 200) {
  replies.push({
    match: (url, m) =>
      url.includes(urlPart) &&
      m === method &&
      (/receipts|deliveries|invoices/.test(urlPart) ||
        !/\/(receipts|deliveries|invoices)/.test(url)),
    status,
    body,
  });
}

const LOCATIONS = [
  {
    id: "l-main",
    code: "MAIN",
    name: "Main warehouse",
    kind: "stock",
    system: false,
    archived: false,
    archivedAt: null,
    createdBy: "u-1",
    createdAt: "2026-08-01T10:00:00Z",
    updatedAt: "2026-08-01T10:00:00Z",
  },
  {
    id: "l-supplier",
    code: "SUPPLIER",
    name: "Suppliers",
    kind: "supplier",
    system: true,
    archived: false,
    archivedAt: null,
    createdBy: "u-1",
    createdAt: "2026-08-01T10:00:00Z",
    updatedAt: "2026-08-01T10:00:00Z",
  },
];

const SUPPLIER = { id: "sup-1", name: "Holz & Söhne", archived: false };

const CUSTOMER = {
  id: "cus-1",
  name: "Acme GmbH",
  addressLine1: "",
  addressLine2: "",
  postalCode: "",
  city: "",
  country: "DE",
  vatId: "",
  email: "billing@acme.test",
  paymentTermsDays: 14,
  currency: "EUR",
  contactId: null,
  archived: false,
  archivedAt: null,
  createdBy: "u-1",
  createdAt: "2026-08-01T10:00:00Z",
  updatedAt: "2026-08-01T10:00:00Z",
};

/** A catalog item whose two prices differ, which is the whole point of it
 *  here: a purchase order that copied the sale price would look right. */
const PRODUCT = {
  id: "p-chair",
  name: "Blue chair",
  unit: "piece",
  unitPriceCents: 9_900,
  vatRateBp: 1900,
  sku: "CH-1",
  barcode: "",
  stocked: true,
  purchasePriceCents: 6_000,
  defaultSupplierId: "sup-1",
  photoNodeId: null,
  archived: false,
  archivedAt: null,
  createdBy: "u-1",
  createdAt: "2026-08-01T10:00:00Z",
  updatedAt: "2026-08-01T10:00:00Z",
};

const PO_DRAFT: PurchaseOrder = {
  id: "po-1",
  supplierId: "sup-1",
  supplierName: "Holz & Söhne",
  status: "draft",
  currency: "EUR",
  number: null,
  orderedDate: null,
  expectedDate: "2026-09-01",
  closedDate: null,
  late: false,
  reference: "Falkenstein",
  note: "",
  createdBy: "u-1",
  createdAt: "2026-08-10T10:00:00Z",
  updatedAt: "2026-08-10T10:00:00Z",
  lines: [
    {
      id: "line-1",
      productId: "p-chair",
      description: "Blue chair",
      unit: "piece",
      qtyMilli: 10_000,
      receivedQtyMilli: 0,
      outstandingQtyMilli: 10_000,
      unitPriceCents: 6_000,
      vatRateBp: 1900,
      netCents: 60_000,
    },
  ],
  totals: {
    netCents: 60_000,
    vatCents: 11_400,
    grossCents: 71_400,
    vatByRate: [{ rateBp: 1900, netCents: 60_000, vatCents: 11_400 }],
  },
};

const PO_PLACED: PurchaseOrder = {
  ...PO_DRAFT,
  status: "sent",
  number: "PO-2026-00004",
  orderedDate: "2026-08-11",
  late: true,
};

const PO_PART: PurchaseOrder = {
  ...PO_PLACED,
  status: "partially_received",
  lines: [
    {
      ...(PO_PLACED.lines[0] as PurchaseOrder["lines"][number]),
      receivedQtyMilli: 4_000,
      outstandingQtyMilli: 6_000,
    },
  ],
};

const RECEIPT = {
  id: "rcp-1",
  sequenceNo: 1,
  locationId: "l-main",
  locationCode: "MAIN",
  locationName: "Main warehouse",
  receivedDate: "2026-08-11",
  note: "one crate damaged",
  billId: "bill-1",
  createdBy: "u-1",
  createdAt: "2026-08-11T09:00:00Z",
  lines: [
    {
      lineId: "line-1",
      productId: "p-chair",
      description: "Blue chair",
      qtyMilli: 4_000,
      moveId: "mv-1",
    },
  ],
};

const SO_PART: SalesOrder = {
  id: "so-1",
  customerId: "cus-1",
  customerName: "Acme GmbH",
  status: "partially_delivered",
  currency: "EUR",
  number: "SO-2026-00002",
  confirmedDate: "2026-08-09",
  expectedDate: "2026-08-20",
  closedDate: null,
  late: false,
  reference: "",
  note: "",
  createdBy: "u-1",
  createdAt: "2026-08-08T10:00:00Z",
  updatedAt: "2026-08-09T10:00:00Z",
  lines: [
    {
      id: "so-line-1",
      productId: "p-chair",
      description: "Blue chair",
      unit: "piece",
      qtyMilli: 10_000,
      deliveredQtyMilli: 7_000,
      outstandingQtyMilli: 3_000,
      invoicedQtyMilli: 2_000,
      // Deliberately NOT 7 − 2: the store's own rule decides what an invoice
      // raised now would carry, and the screen must print that figure rather
      // than a subtraction of its own.
      invoiceableQtyMilli: 5_000,
      unitPriceCents: 9_900,
      vatRateBp: 1900,
      netCents: 99_000,
    },
  ],
  totals: {
    netCents: 99_000,
    vatCents: 18_810,
    grossCents: 117_810,
    vatByRate: [{ rateBp: 1900, netCents: 99_000, vatCents: 18_810 }],
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
      : url.includes("/inventory/locations")
        ? { locations: LOCATIONS }
        : url.includes("/inventory/suppliers")
          ? { suppliers: [SUPPLIER] }
          : url.includes("/billing/products")
            ? { products: [PRODUCT] }
            : url.includes("/billing/customers")
              ? { customers: [CUSTOMER] }
              : url.includes("/receipts")
                ? { receipts: [] }
                : url.includes("/deliveries")
                  ? { deliveries: [] }
                  : url.includes("/sales-orders")
                    ? { salesOrders: [], invoices: [] }
                    : url.includes("/purchase-orders")
                      ? { purchaseOrders: [] }
                      : url.includes("/audit")
                        ? { entries: [] }
                        : {};
  return { match: () => true, status: 200, body };
}

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

/** The module as it is really mounted: at `/inventory/*`, routing itself. */
function ui(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <DialogProvider>
        <Routes>
          <Route path="/inventory/*" element={<InventoryModule />} />
        </Routes>
      </DialogProvider>
    </MemoryRouter>,
  );
}

/** The last write the client made, if it made one. */
function lastWrite(): Call | undefined {
  return calls.filter((c) => c.method !== "GET").at(-1);
}

/** Answers a confirmation dialog. Its confirm button carries the same label as
 *  the action that opened it, and it is rendered after the page, so the last
 *  one is the dialog's. */
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

describe("the purchasing list", () => {
  test("shows the server's number, supplier, state and late flag", async () => {
    reply("/inventory/purchase-orders", "GET", { purchaseOrders: [PO_PLACED] });
    ui("/inventory/purchase-orders");

    expect(await screen.findByText("PO-2026-00004")).toBeTruthy();
    const table = within(screen.getByRole("table"));
    expect(table.getByText("Holz & Söhne")).toBeTruthy();
    expect(table.getByText("€714.00")).toBeTruthy();
    // The chips, not the filter's options, which carry the same words.
    expect(table.getByText(strings.inventoryPoStatusSent)).toBeTruthy();
    // Late is the server's flag; nothing here compared a date to today.
    expect(table.getByText(strings.inventoryOrderLate)).toBeTruthy();
  });

  test("the state filter asks the server rather than narrowing a loaded page", async () => {
    reply("/inventory/purchase-orders", "GET", { purchaseOrders: [PO_DRAFT] });
    ui("/inventory/purchase-orders");
    await screen.findByRole("button", { name: strings.inventoryDraftOrder });

    reply("/inventory/purchase-orders", "GET", { purchaseOrders: [PO_PLACED] });
    fireEvent.click(screen.getByRole("combobox", { name: strings.inventoryFilterStatus }));
    fireEvent.click(screen.getByRole("option", { name: strings.inventoryPoStatusSent }));

    await waitFor(() =>
      expect(
        calls.some((c) => c.url.includes("/inventory/purchase-orders?status=sent")),
      ).toBe(true),
    );
  });
});

describe("a purchase-order draft", () => {
  test("picking a catalog item copies what we pay, and the line carries the link", async () => {
    reply("/inventory/purchase-orders/po-1", "GET", { purchaseOrder: { ...PO_DRAFT, lines: [] } });
    ui("/inventory/purchase-orders/po-1");

    await screen.findByText(strings.inventoryLines);
    fireEvent.click(screen.getByRole("button", { name: strings.inventoryAddLine }));
    fireEvent.change(screen.getByLabelText(strings.inventoryPickProduct), {
      target: { value: "p-chair" },
    });

    // 60 is the purchase price; 99 is what we charge, and it must not appear
    // anywhere on an order we are placing.
    const price = screen.getByLabelText(strings.inventoryColUnitPrice) as HTMLInputElement;
    expect(price.value).toBe("60");

    fireEvent.change(screen.getByLabelText(strings.inventoryColQuantity), {
      target: { value: "10" },
    });
    reply("/inventory/purchase-orders/po-1", "PATCH", { purchaseOrder: PO_DRAFT });
    fireEvent.click(screen.getByRole("button", { name: strings.inventorySaveDraft }));

    await waitFor(() => expect(lastWrite()?.method).toBe("PATCH"));
    const body = lastWrite()?.body as { lines: unknown[] };
    expect(body.lines).toEqual([
      {
        productId: "p-chair",
        description: "Blue chair",
        unit: "piece",
        qtyMilli: 10_000,
        unitPriceCents: 6_000,
        vatRateBp: 1900,
      },
    ]);
  });

  test("a row that is not a line stops the save instead of being dropped from it", async () => {
    reply("/inventory/purchase-orders/po-1", "GET", { purchaseOrder: { ...PO_DRAFT, lines: [] } });
    ui("/inventory/purchase-orders/po-1");

    await screen.findByText(strings.inventoryLines);
    fireEvent.click(screen.getByRole("button", { name: strings.inventoryAddLine }));
    fireEvent.change(screen.getByLabelText(strings.inventoryColQuantity), {
      target: { value: "two" },
    });
    fireEvent.click(screen.getByRole("button", { name: strings.inventorySaveDraft }));

    expect(await screen.findByText(strings.inventoryFixLinesFirst)).toBeTruthy();
    expect(lastWrite()).toBeUndefined();
  });
});

describe("placing an order", () => {
  test("says all three things it does, and afterwards the document is frozen", async () => {
    reply("/inventory/purchase-orders/po-1", "GET", { purchaseOrder: PO_DRAFT });
    ui("/inventory/purchase-orders/po-1");
    await screen.findByText(strings.inventoryLines);

    fireEvent.click(screen.getByRole("button", { name: strings.inventorySendOrder }));
    // The number, the freeze and the letter — all three, before anything.
    expect(await screen.findByText(strings.inventorySendOrderConfirm)).toBeTruthy();

    reply("/inventory/purchase-orders/po-1/send", "POST", {
      purchaseOrder: PO_PLACED,
      draft: {
        id: "draft-9",
        to: "orders@holz.test",
        subject: "Purchase order PO-2026-00004",
        attachment: { name: "PO-2026-00004.pdf", sizeBytes: 40_000 },
      },
    });
    press(strings.inventorySendOrder);

    await screen.findByText("PO-2026-00004");
    expect(
      calls.some((c) => c.method === "POST" && c.url.includes("/po-1/send?lang=")),
    ).toBe(true);
    // The letter is named and said to be unsent, which is the whole promise.
    expect(
      screen.getByText(
        strings.inventoryOrderPlacedNotice("orders@holz.test", "PO-2026-00004.pdf"),
      ),
    ).toBeTruthy();
    expect(screen.getByText(strings.inventoryOrderFrozenNotice)).toBeTruthy();
    // A placed order offers nothing to type into: not the header, not a line.
    expect(screen.queryByLabelText(strings.inventoryColUnitPrice)).toBeNull();
    expect(screen.queryByRole("button", { name: strings.inventorySaveDraft })).toBeNull();
  });
});

describe("booking an arrival", () => {
  test("opens on what is outstanding and books what was typed over it", async () => {
    reply("/inventory/purchase-orders/po-1", "GET", { purchaseOrder: PO_PLACED });
    ui("/inventory/purchase-orders/po-1");
    await screen.findByText("PO-2026-00004");

    fireEvent.click(screen.getByRole("button", { name: strings.inventoryReceiveGoods }));
    const sheet = within(await screen.findByRole("dialog"));
    const qty = sheet.getByLabelText(`${strings.inventoryColThisConsignment} — Blue chair`, {
      exact: false,
    }) as HTMLInputElement;
    // Everything still owing, so the ordinary case is one click.
    expect(qty.value).toBe("10");

    fireEvent.change(qty, { target: { value: "4" } });
    reply("/inventory/purchase-orders/po-1/receipts", "POST", {
      purchaseOrder: PO_PART,
      receipt: RECEIPT,
      billId: "bill-1",
    });
    reply("/inventory/purchase-orders/po-1/receipts", "GET", { receipts: [RECEIPT] });
    fireEvent.click(sheet.getByRole("button", { name: strings.inventoryBookArrival }));

    await waitFor(() =>
      expect(calls.some((c) => c.method === "POST" && c.url.includes("/receipts"))).toBe(true),
    );
    const booked = calls.find((c) => c.method === "POST" && c.url.includes("/receipts"));
    expect(booked?.body).toEqual({
      locationId: "l-main",
      lines: [{ lineId: "line-1", qtyMilli: 4_000 }],
    });

    // The document now says what came and what is still owed — both the
    // server's own figures — and the arrival is listed with its bill.
    expect(await screen.findByText(strings.inventoryArrivalNo(1))).toBeTruthy();
    expect(screen.getByText(strings.inventoryBillDrafted)).toBeTruthy();
    const lines = within(screen.getAllByRole("table")[0] as HTMLTableElement);
    expect(lines.getByText("6")).toBeTruthy();
  });

  test("a refusal is shown in the server's words with the sheet still open", async () => {
    reply("/inventory/purchase-orders/po-1", "GET", { purchaseOrder: PO_PLACED });
    ui("/inventory/purchase-orders/po-1");
    await screen.findByText("PO-2026-00004");

    fireEvent.click(screen.getByRole("button", { name: strings.inventoryReceiveGoods }));
    const sheet = within(await screen.findByRole("dialog"));
    reply(
      "/inventory/purchase-orders/po-1/receipts",
      "POST",
      { detail: "line line-1 has only 6 outstanding" },
      409,
    );
    fireEvent.click(sheet.getByRole("button", { name: strings.inventoryBookArrival }));

    expect(await sheet.findByText("line line-1 has only 6 outstanding")).toBeTruthy();
    // Still open, still holding what was typed: a refusal is something to
    // correct, not a form to fill in again.
    expect(screen.getByRole("dialog")).toBeTruthy();
  });
});

describe("a sales order", () => {
  test("prints the server's invoiceable quantity and raises a draft invoice", async () => {
    reply("/inventory/sales-orders/so-1", "GET", { salesOrder: SO_PART });
    ui("/inventory/sales-orders/so-1");
    await screen.findByText("SO-2026-00002");

    const lines = within(screen.getAllByRole("table")[0] as HTMLTableElement);
    // Delivered 7, outstanding 3, to bill 5 — and 5 is the server's, not
    // 7 − 2. A browser that subtracted would print 5 here too; the fixture is
    // built so that it would print 5 only by accident, and the assertion below
    // pins the column to the field it came from.
    expect(lines.getByText(strings.inventoryColToBill)).toBeTruthy();
    expect(lines.getByText("7")).toBeTruthy();
    expect(lines.getByText("3")).toBeTruthy();
    expect(lines.getByText("5")).toBeTruthy();

    reply("/inventory/sales-orders/so-1/invoice", "POST", {
      salesOrder: {
        ...SO_PART,
        lines: [{ ...SO_PART.lines[0], invoicedQtyMilli: 7_000, invoiceableQtyMilli: 0 }],
      },
      invoice: {
        id: "soi-1",
        invoiceId: "inv-9",
        invoiceNumber: null,
        invoiceStatus: "draft",
        createdBy: "u-1",
        createdAt: "2026-08-11T12:00:00Z",
        lines: [{ lineId: "so-line-1", qtyMilli: 5_000 }],
      },
    });
    reply("/inventory/sales-orders/so-1/invoices", "GET", {
      invoices: [
        {
          id: "soi-1",
          invoiceId: "inv-9",
          invoiceNumber: null,
          invoiceStatus: "draft",
          createdBy: "u-1",
          createdAt: "2026-08-11T12:00:00Z",
          lines: [{ lineId: "so-line-1", qtyMilli: 5_000 }],
        },
      ],
    });
    fireEvent.click(screen.getByRole("button", { name: strings.inventoryRaiseInvoice }));

    // A draft, said to be a draft: it has drawn nothing from the gapless series.
    expect(await screen.findByText(strings.inventoryInvoiceDrafted)).toBeTruthy();
    expect(screen.getByRole("button", { name: strings.inventoryDraftInvoice })).toBeTruthy();
    // Nothing left to bill, so the button that would bill it is gone.
    expect(screen.queryByRole("button", { name: strings.inventoryRaiseInvoice })).toBeNull();
  });
});
