// What the scanner promises, proven against a recorded network.
//
// Six claims, each of them something a scanning screen can silently get wrong
// in a warehouse:
//
// - a **keyboard-wedge scanner works with no camera and no permission**: it
//   types the digits into the focused field and presses Enter, and that alone
//   asks the server;
// - the answer is the **server's** — the product, the places, and the quantity
//   it folded from the ledger, with nothing added up here;
// - a **misread code and an unknown product are different answers**, shown in
//   the server's own sentence, and only the unknown one offers to add a
//   product;
// - **a hit clears the field**, because a wedge scanner types into whatever has
//   focus and leftover digits would be prefixed to the next scan;
// - the **camera is offered only where the browser can do it**, and detecting a
//   code through it asks exactly the same question as typing one;
// - **a service carries no quantity**, not a zero.
//
// Only the network and `BarcodeDetector` are fake. The real router, the real
// module routes, the real client and the real i18n catalog all run.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import { InventoryModule } from "./InventoryModule";

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

const CONSULTING = { ...CHAIR, id: "p-adv", name: "Consulting", sku: "", barcode: "", stocked: false };

/** The chair in two warehouses, as the scan route answers: real places only. */
const SCANNED_STOCK = [
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

interface Call {
  url: string;
  method: string;
}

const calls: Call[] = [];

/** What the scan route answers next. Replaced per test. */
let scanAnswer: () => Response = () =>
  json({
    code: "4006381333931",
    product: CHAIR,
    stock: SCANNED_STOCK,
    // The server's own fold: 4000 + 1500. A screen that recomputed it could
    // disagree with the ledger, so this number is deliberately NOT the sum of
    // any rows a test could add up by hand elsewhere.
    onHandQtyMilli: 5_500,
  });

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
  calls.push({ url, method: init?.method ?? "GET" });
  if (url.includes("/inventory/scan")) return scanAnswer();
  if (url.includes("/billing/products")) return json({ products: [CHAIR, CONSULTING] });
  if (url.includes("/inventory/stock")) return json({ stock: SCANNED_STOCK, totalValueCents: 33_000 });
  if (url.includes("/inventory/locations")) {
    return json({
      locations: [
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
      ],
    });
  }
  if (url.includes("/inventory/suppliers")) return json({ suppliers: [] });
  return json({});
});

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

function ui(at = "/inventory/catalog") {
  return render(
    <MemoryRouter initialEntries={[at]}>
      <DialogProvider>
        <Routes>
          <Route path="/inventory/*" element={<InventoryModule />} />
        </Routes>
      </DialogProvider>
    </MemoryRouter>,
  );
}

/** Opens the scanner from a screen's toolbar and returns its dialog. */
async function openScanner(at?: string) {
  ui(at);
  fireEvent.click(await screen.findByRole("button", { name: strings.inventoryScan }));
  return screen.findByRole("dialog");
}

/** What a keyboard-wedge scanner does: types the whole code into the focused
 *  field and presses Enter. It is a keyboard, and this is the whole of it. */
function wedgeScan(dialog: HTMLElement, code: string) {
  const field = within(dialog).getByLabelText(strings.inventoryScanFieldCode, { exact: false });
  fireEvent.change(field, { target: { value: code } });
  fireEvent.submit(field.closest("form") as HTMLFormElement);
  return field as HTMLInputElement;
}

/** The scan request the client made, if it made one. */
function scanCall() {
  return calls.find((call) => call.url.includes("/inventory/scan"));
}

beforeEach(() => {
  calls.length = 0;
  fakeFetch.mockClear();
  scanAnswer = () =>
    json({ code: "4006381333931", product: CHAIR, stock: SCANNED_STOCK, onHandQtyMilli: 5_500 });
  // The default browser: no barcode detector, which is Safari and Firefox
  // today. Every test that wants a camera says so.
  delete (window as unknown as { BarcodeDetector?: unknown }).BarcodeDetector;
});

afterEach(cleanup);

describe("scanning a barcode", () => {
  test("a wedge scanner's digits and Enter are the whole act", async () => {
    const dialog = await openScanner();
    const field = wedgeScan(dialog, "4006381333931");

    await waitFor(() => {
      expect(scanCall()).toBeDefined();
    });
    expect(scanCall()?.url).toContain("code=4006381333931");
    expect(scanCall()?.method).toBe("GET");

    // The answer is the server's: its product, its places, its fold.
    expect(await within(dialog).findByText("Blue chair")).toBeTruthy();
    expect(within(dialog).getByText(strings.inventoryScanOnHand("5.5"))).toBeTruthy();
    expect(within(dialog).getByText("MAIN")).toBeTruthy();
    expect(within(dialog).getByText("VAN1")).toBeTruthy();

    // Cleared, so the next scan is not appended to this one.
    await waitFor(() => {
      expect(field.value).toBe("");
    });
  });

  test("separators are the label's, and the code sent is what was read", async () => {
    const dialog = await openScanner();
    wedgeScan(dialog, "  400-638 133 393 1 ");
    await waitFor(() => {
      expect(scanCall()).toBeDefined();
    });
    // Trimmed but not otherwise rewritten: the server owns what a code is, and
    // canonicalising one here would be a second opinion about it. The spaces
    // travel as a query string's `+`, which is what the server reads them back
    // from.
    expect(scanCall()?.url).toContain("code=400-638+133+393+1");
  });

  test("a misread code is refused in the server's words, with nothing to add", async () => {
    scanAnswer = () =>
      json({ detail: "the check digit of this barcode does not match; check for a typo" }, 422);
    const dialog = await openScanner();
    const field = wedgeScan(dialog, "4006381333930");

    expect(
      await within(dialog).findByText(
        "the check digit of this barcode does not match; check for a typo",
      ),
    ).toBeTruthy();
    // A mangled code must never become a product's barcode.
    expect(within(dialog).queryByRole("button", { name: strings.inventoryScanAddProduct })).toBeNull();
    // The digits stay, because they are worth reading before the next scan.
    expect(field.value).toBe("4006381333930");
  });

  test("a real code nobody stocks offers to add the thing it is on", async () => {
    scanAnswer = () => json({ detail: "no product in this catalog carries this barcode" }, 404);
    const dialog = await openScanner();
    wedgeScan(dialog, "4006381333931");

    const add = await within(dialog).findByRole("button", {
      name: strings.inventoryScanAddProduct,
    });
    fireEvent.click(add);

    // The editor opens on a NEW product with the scanned code already in it.
    const editor = await screen.findByRole("dialog");
    const barcode = within(editor).getByLabelText(strings.inventoryFieldBarcode, {
      exact: false,
    }) as HTMLInputElement;
    expect(barcode.value).toBe("4006381333931");
  });

  test("a service carries no quantity at all", async () => {
    scanAnswer = () =>
      json({ code: "4006381333931", product: CONSULTING, stock: [], onHandQtyMilli: 0 });
    const dialog = await openScanner();
    wedgeScan(dialog, "4006381333931");

    expect(await within(dialog).findByText(strings.inventoryScanServiceNote)).toBeTruthy();
    expect(within(dialog).queryByText(strings.inventoryScanOnHand("0"))).toBeNull();
  });

  test("the camera is offered only where the browser can read one", async () => {
    const withoutCamera = await openScanner();
    expect(
      within(withoutCamera).queryByRole("button", { name: strings.inventoryScanCameraStart }),
    ).toBeNull();
    expect(within(withoutCamera).getByText(strings.inventoryScanNoCamera)).toBeTruthy();
    cleanup();

    // A browser that has both halves of it: the detector and a camera.
    (window as unknown as { BarcodeDetector: unknown }).BarcodeDetector = class {
      detect() {
        return Promise.resolve([{ rawValue: "4006381333931" }]);
      }
    };
    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: { getUserMedia: () => Promise.resolve({ getTracks: () => [] }) },
    });

    const withCamera = await openScanner();
    expect(
      within(withCamera).getByRole("button", { name: strings.inventoryScanCameraStart }),
    ).toBeTruthy();
    expect(within(withCamera).queryByText(strings.inventoryScanNoCamera)).toBeNull();
  });

  test("the stock screen scans too, and the scan finds the row", async () => {
    const dialog = await openScanner("/inventory/stock");
    wedgeScan(dialog, "4006381333931");

    const show = await within(dialog).findByRole("button", {
      name: strings.inventoryScanShowInStock,
    });
    fireEvent.click(show);

    // The list is filtered by the product's own code — what a person would
    // have typed with the box in front of them.
    const search = screen.getByLabelText(strings.inventorySearchStock) as HTMLInputElement;
    expect(search.value).toBe("CH-1");
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
