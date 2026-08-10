// What the catalog screen promises, proven against a recorded network.
//
// Four claims, each of them something a catalog can silently get wrong about a
// warehouse:
//
// - a product is shown **as a thing**: its code, the code on the box, and how
//   much of it there is *across every place*, added from the ledger's own rows;
// - a **service has no quantity at all** — not a zero, which would read as an
//   empty shelf for something that can never be on one;
// - the editor is **Billing's product dialog**, and it sends only what changed,
//   so opening a product and saving it cannot rewrite a price nobody touched;
// - a refusal — a barcode whose check digit is wrong — is shown in the
//   **server's own words**, and the form stays open on it.
//
// Only the network is fake. The real router, the real module routes, the real
// clients and the real i18n catalog all run.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import { InventoryModule } from "./InventoryModule";

interface Call {
  url: string;
  method: string;
  body: unknown;
}

const calls: Call[] = [];

/** Something on a shelf, in two places. */
const CHAIR = {
  id: "p-chair",
  name: "Blue chair",
  unit: "piece",
  unitPriceCents: 12_500,
  vatRateBp: 2100,
  sku: "CH-1",
  barcode: "4006381333931",
  stocked: true,
  purchasePriceCents: 6_000,
  photoNodeId: null,
  defaultSupplierId: null,
  archived: false,
  archivedAt: null,
  createdBy: "u-1",
  createdAt: "2026-08-01T10:00:00Z",
  updatedAt: "2026-08-01T10:00:00Z",
};

/** Something that can never be on one. */
const CONSULTING = { ...CHAIR, id: "p-adv", name: "Consulting", unit: "hour", sku: "", barcode: "", stocked: false };

/** The same chair in two warehouses: four here, one and a half there. */
const STOCK = [
  {
    productId: "p-chair",
    productName: "Blue chair",
    sku: "CH-1",
    locationId: "l-main",
    locationCode: "MAIN",
    locationName: "Main warehouse",
    locationKind: "stock",
    real: true,
    qtyMilli: 4_000,
    valueCents: 24_000,
    lastMoveAt: "2026-08-09T09:00:00Z",
  },
  {
    productId: "p-chair",
    productName: "Blue chair",
    sku: "CH-1",
    locationId: "l-van",
    locationCode: "VAN1",
    locationName: "Van",
    locationKind: "stock",
    real: true,
    qtyMilli: 1_500,
    valueCents: 9_000,
    lastMoveAt: "2026-08-09T11:00:00Z",
  },
];

let products = [CHAIR, CONSULTING];
/** What a write answers with. Replaced per test. */
let writeAnswer: () => Response = () => json({ product: CHAIR });

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
  const method = init?.method ?? "GET";
  calls.push({
    url,
    method,
    body: typeof init?.body === "string" ? JSON.parse(init.body) : undefined,
  });
  if (url.includes("/billing/products")) {
    if (method === "GET") return json({ products });
    return writeAnswer();
  }
  if (url.includes("/inventory/stock")) return json({ stock: STOCK, totalValueCents: 33_000 });
  if (url.includes("/inventory/suppliers")) {
    return json({ suppliers: [{ id: "s-1", name: "Meubelgroothandel", archived: false }] });
  }
  return json({});
});

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

function ui() {
  return render(
    <MemoryRouter initialEntries={["/inventory/catalog"]}>
      <DialogProvider>
        <Routes>
          <Route path="/inventory/*" element={<InventoryModule />} />
        </Routes>
      </DialogProvider>
    </MemoryRouter>,
  );
}

/** Renders the tab and opens the editor of one product by its name button. */
async function openEditor(name: string) {
  ui();
  fireEvent.click(await screen.findByRole("button", { name }));
  return screen.findByRole("dialog");
}

beforeEach(() => {
  calls.length = 0;
  products = [CHAIR, CONSULTING];
  writeAnswer = () => json({ product: CHAIR });
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("the catalog", () => {
  test("shows a product as a thing, with what is on hand everywhere", async () => {
    ui();
    const row = (await screen.findByRole("button", { name: "Blue chair" })).closest("tr");
    expect(row).not.toBeNull();
    const cells = within(row as HTMLTableRowElement);
    expect(cells.getByText("CH-1")).toBeTruthy();
    expect(cells.getByText("4006381333931")).toBeTruthy();
    // 4000 + 1500 milli-units, from the two warehouse rows the server sent.
    expect(cells.getByText("5.5")).toBeTruthy();
    expect(cells.getByText(strings.inventoryTypeStocked, { exact: false })).toBeTruthy();
  });

  test("a service carries no quantity at all", async () => {
    ui();
    const row = (await screen.findByRole("button", { name: "Consulting" })).closest("tr");
    const cells = within(row as HTMLTableRowElement);
    expect(cells.getByText(strings.inventoryNotStocked)).toBeTruthy();
    expect(cells.queryByText("0")).toBeNull();
    expect(cells.getByText(strings.inventoryTypeService, { exact: false })).toBeTruthy();
  });

  test("the editor sends only what changed", async () => {
    const dialog = await openEditor("Blue chair");
    const form = within(dialog);
    fireEvent.change(form.getByLabelText(strings.inventoryFieldSku, { exact: false }), {
      target: { value: "CH-2" },
    });
    fireEvent.change(form.getByLabelText(strings.inventoryFieldDefaultSupplier, { exact: false }), {
      target: { value: "s-1" },
    });
    fireEvent.click(form.getByRole("button", { name: strings.billingSave }));

    await waitFor(() => {
      expect(calls.some((call) => call.method === "PATCH")).toBe(true);
    });
    const patch = calls.find((call) => call.method === "PATCH");
    expect(patch?.url).toContain("/billing/products/p-chair");
    // Exactly the two fields touched — no price, no rate, no barcode, and
    // nothing that would rewrite what the product costs.
    expect(patch?.body).toEqual({ sku: "CH-2", defaultSupplierId: "s-1" });
  });

  test("a refused barcode is shown in the server's words, and the form stays", async () => {
    writeAnswer = () =>
      json({ detail: "that barcode's check digit does not match" }, 422);
    const dialog = await openEditor("Blue chair");
    const form = within(dialog);
    fireEvent.change(form.getByLabelText(strings.inventoryFieldBarcode, { exact: false }), {
      target: { value: "4006381333930" },
    });
    fireEvent.click(form.getByRole("button", { name: strings.billingSave }));

    expect(
      await within(dialog).findByText("that barcode's check digit does not match"),
    ).toBeTruthy();
    expect(screen.getByRole("dialog")).toBeTruthy();
  });
});
