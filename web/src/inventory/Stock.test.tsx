// What the stock screen promises, proven against a recorded network.
//
// Four claims, each of them something a stock screen can silently get wrong:
//
// - on-hand is shown **per place**, with the value the server computed and
//   never a figure the browser multiplied;
// - the total is labelled as a **reference figure at purchase prices**, because
//   B5 chooses no costing method and a number under the word "value" that
//   nobody qualified is read as a balance;
// - the **counterparties are off** until asked for, and asking says what it
//   does to the total — a closed ledger sums to roughly nothing;
// - the **history** behind a row says from → to, why, and which document, which
//   is the answer to the question the missing quantity field would have asked.
//
// Only the network is fake. The real router, the real module routes, the real
// client and the real i18n catalog all run.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import { InventoryModule } from "./InventoryModule";

interface Call {
  url: string;
  method: string;
}

const calls: Call[] = [];

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

const SHELF = {
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
};

const COUNTERPARTY = {
  ...SHELF,
  locationId: "l-supplier",
  locationCode: "SUPPLIER",
  locationName: "Suppliers",
  locationKind: "supplier",
  real: false,
  qtyMilli: -4_000,
  valueCents: -24_000,
};

const MOVE = {
  id: "m-1",
  productId: "p-chair",
  productName: "Blue chair",
  fromLocationId: "l-supplier",
  fromCode: "SUPPLIER",
  fromName: "Suppliers",
  toLocationId: "l-main",
  toCode: "MAIN",
  toName: "Main warehouse",
  qtyMilli: 4_000,
  reason: "receipt",
  reasonCode: null,
  note: null,
  refKind: "po",
  refId: "po-7",
  occurredAt: "2026-08-09T09:00:00Z",
  createdBy: "u-1",
  createdAt: "2026-08-09T09:00:00Z",
};

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

/** What the movement read answers with. Replaced per test. */
let movesAnswer: () => Response = () => json({ moves: [MOVE], limit: 200 });

const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
  calls.push({ url, method: init?.method ?? "GET" });
  if (url.includes("/inventory/locations")) return json({ locations: LOCATIONS });
  if (url.includes("/inventory/moves")) return movesAnswer();
  if (url.includes("/inventory/stock")) {
    const virtual = url.includes("includeVirtual=1");
    return virtual
      ? json({ stock: [SHELF, COUNTERPARTY], totalValueCents: 0 })
      : json({ stock: [SHELF], totalValueCents: 24_000 });
  }
  return json({});
});

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

function ui() {
  return render(
    <MemoryRouter initialEntries={["/inventory/stock"]}>
      <DialogProvider>
        <Routes>
          <Route path="/inventory/*" element={<InventoryModule />} />
        </Routes>
      </DialogProvider>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  calls.length = 0;
  movesAnswer = () => json({ moves: [MOVE], limit: 200 });
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("the stock screen", () => {
  test("shows what is where, and calls the total what it is", async () => {
    ui();
    const row = (await screen.findByText("Blue chair")).closest("tr");
    const cells = within(row as HTMLTableRowElement);
    expect(cells.getByText("MAIN")).toBeTruthy();
    expect(cells.getByText("4")).toBeTruthy();
    expect(cells.getByText("240.00")).toBeTruthy();
    // The server's own total, under the words that say it is not a balance.
    expect(screen.getByText(strings.inventoryReferenceValue("240.00"))).toBeTruthy();
  });

  test("the counterparties are off until asked for, and asking says why", async () => {
    ui();
    await screen.findByText("Blue chair");
    expect(screen.queryByText(strings.inventoryCounterpartiesNote)).toBeNull();
    expect(calls.some((call) => call.url.includes("includeVirtual=1"))).toBe(false);

    fireEvent.click(screen.getByLabelText(strings.inventoryShowCounterparties));

    expect(await screen.findByText(strings.inventoryCounterpartiesNote)).toBeTruthy();
    await waitFor(() => {
      expect(calls.some((call) => call.url.includes("includeVirtual=1"))).toBe(true);
    });
    expect(await screen.findByText("SUPPLIER")).toBeTruthy();
  });

  test("there is no way to type a quantity — the history says where it went", async () => {
    ui();
    await screen.findByText("Blue chair");
    // Not a single editable field on the whole list: a quantity changes here by
    // something happening, never by being typed.
    expect(screen.queryByRole("spinbutton")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: strings.inventoryOpenHistory }));
    const dialog = await screen.findByRole("dialog");
    const history = within(dialog);
    expect(history.getByText("SUPPLIER → MAIN")).toBeTruthy();
    expect(history.getByText(strings.inventoryReasonReceipt)).toBeTruthy();
    expect(history.getByText("po-7")).toBeTruthy();
    // Filtered to the row that was clicked, at either end.
    expect(
      calls.some(
        (call) =>
          call.url.includes("/inventory/moves") &&
          call.url.includes("productId=p-chair") &&
          call.url.includes("locationId=l-main"),
      ),
    ).toBe(true);
  });

  test("a refused history read is shown in the server's words", async () => {
    movesAnswer = () => json({ detail: "from must be an RFC 3339 timestamp" }, 422);
    ui();
    await screen.findByText("Blue chair");
    fireEvent.click(screen.getByRole("button", { name: strings.inventoryOpenHistory }));
    const dialog = await screen.findByRole("dialog");
    expect(
      await within(dialog).findByText("from must be an RFC 3339 timestamp"),
    ).toBeTruthy();
  });
});
