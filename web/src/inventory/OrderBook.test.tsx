// What the order book promises, proven against a recorded network.
//
// Five claims, each of them something this screen could silently get wrong:
//
//  - every figure printed is the **server's**, including the totals — the
//    fixture's totals deliberately do not equal the sum of its rows, so a
//    screen that added up its own would fail here;
//  - changing the scope **asks the server again**, rather than narrowing a
//    page it already holds — the server decides what "open" means;
//  - a book spanning two currencies **withholds the total and says why**,
//    instead of printing the sum of euros and pounds;
//  - what is still owed in *goods* is shown beside what is owed in *money*,
//    and only where there are goods to move;
//  - a row is a way into its order.
//
// Only the network is fake. The real router, the real module routes, the real
// client and the real i18n catalog all run.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import { InventoryModule } from "./InventoryModule";
import type { OrderBookFigures } from "./types";

interface Call {
  url: string;
  method: string;
}

const calls: Call[] = [];
let replies: { match: (url: string) => boolean; status: number; body: unknown }[] = [];

/** Queues one answer for the next `GET` whose URL contains `urlPart`. */
function reply(urlPart: string, body: unknown, status = 200) {
  replies.push({ match: (url) => url.includes(urlPart), status, body });
}

/** Figures with everything at zero, so each fixture states only what it means. */
function figures(over: Partial<OrderBookFigures>): OrderBookFigures {
  return {
    orderedNetCents: 0,
    reservedNetCents: 0,
    deliveredNetCents: 0,
    invoicedNetCents: 0,
    outstandingNetCents: 0,
    orderedQtyMilli: 0,
    reservedQtyMilli: 0,
    deliveredQtyMilli: 0,
    outstandingQtyMilli: 0,
    ...over,
  };
}

const GOODS = {
  id: "so-1",
  number: "SO-2026-00004",
  customerId: "c-1",
  customerName: "Acme GmbH",
  status: "partially_delivered",
  currency: "EUR",
  // Five distinct amounts, and outstanding is deliberately **not** ordered
  // minus delivered: the server is entitled to know something this screen does
  // not, and the screen's job is to print what it was told.
  figures: figures({
    orderedNetCents: 100_000,
    reservedNetCents: 40_000,
    deliveredNetCents: 60_000,
    invoicedNetCents: 25_000,
    outstandingNetCents: 35_000,
    orderedQtyMilli: 10_000,
    deliveredQtyMilli: 6_000,
    outstandingQtyMilli: 4_000,
  }),
};

/** An order of pure services: worth money, and no goods will ever move for it. */
const SERVICES = {
  id: "so-2",
  number: "SO-2026-00005",
  customerId: "c-2",
  customerName: "Bureau Dupont",
  status: "confirmed",
  currency: "EUR",
  figures: figures({ orderedNetCents: 50_000, outstandingNetCents: 50_000 }),
};

/**
 * Totals that are **not** the sum of the two rows above (which would be
 * €1,500.00 ordered and €900.00 outstanding).
 *
 * That is the point of the fixture, not a mistake in it: the store's totals
 * come from its own query and a short-closed order is owed nothing while the
 * subtraction still says it is. If this screen ever starts adding up its rows,
 * this is the test that notices.
 */
const TOTALS = figures({
  orderedNetCents: 111_100,
  reservedNetCents: 22_200,
  deliveredNetCents: 33_300,
  invoicedNetCents: 44_400,
  outstandingNetCents: 55_500,
});

/**
 * What an unqueued read answers.
 *
 * The order document matters here even though this file is not about it:
 * clicking a row really navigates, the real editor really mounts, and a body
 * without a `salesOrder` in it crashes that editor — which would show up as an
 * unhandled error beside a passing test rather than as a failure.
 */
function fallback(url: string): unknown {
  if (/\/sales-orders\/[^/]+$/.test(url)) {
    return { salesOrder: { ...GOODS, ...EMPTY_ORDER_HEADER, lines: [] } };
  }
  if (url.includes("/billing/customers")) return { customers: [] };
  if (url.includes("/billing/products")) return { products: [] };
  if (url.includes("/inventory/locations")) return { locations: [] };
  if (url.includes("/deliveries")) return { deliveries: [] };
  if (url.includes("/invoices")) return { invoices: [] };
  return { orders: [], totals: figures({}), currencies: [], scope: "open" };
}

/** The header fields a book row does not carry but a document has. */
const EMPTY_ORDER_HEADER = {
  confirmedDate: "2026-08-10",
  expectedDate: null,
  closedDate: null,
  late: false,
  reference: "",
  note: "",
  createdBy: "u-1",
  createdAt: "2026-08-10T10:00:00Z",
  updatedAt: "2026-08-10T10:00:00Z",
  totals: { netCents: 100_000, vatCents: 0, grossCents: 100_000, vatByRate: [] },
};

const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
  const method = init?.method ?? "GET";
  calls.push({ url, method });
  const index = replies.findIndex((r) => r.match(url));
  const answer = index === -1 ? { status: 200, body: fallback(url) } : replies.splice(index, 1)[0];
  return new Response(JSON.stringify(answer?.body), {
    status: answer?.status ?? 200,
    headers: { "content-type": "application/json" },
  });
});

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

/** The module as it is really mounted, so the tab and the route are the real
 *  ones and not a harness's idea of them. */
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

/** The URLs the client actually asked for, in order. */
function reads(): string[] {
  return calls.filter((c) => c.method === "GET").map((c) => c.url);
}

beforeEach(() => {
  calls.length = 0;
  replies = [];
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("the order book", () => {
  test("prints the server's figures for each order, and the server's own totals", async () => {
    reply("/inventory/order-book", {
      scope: "open",
      orders: [GOODS, SERVICES],
      totals: TOTALS,
      currencies: ["EUR"],
    });
    ui("/inventory/order-book");

    expect(await screen.findByText("SO-2026-00004")).toBeTruthy();
    const table = within(screen.getByRole("table"));
    expect(table.getByText("Acme GmbH")).toBeTruthy();
    // The five figures of the part-delivered order, as the server sent them —
    // note €350.00 outstanding where the subtraction would have said €400.00.
    expect(table.getByText("€1,000.00")).toBeTruthy();
    expect(table.getByText("€400.00")).toBeTruthy();
    expect(table.getByText("€600.00")).toBeTruthy();
    expect(table.getByText("€250.00")).toBeTruthy();
    expect(table.getByText("€350.00")).toBeTruthy();

    // And the totals, which are the store's answer and not this page's
    // addition — €1,111.00 is not €1,000.00 + €500.00.
    expect(table.getByText(strings.inventoryBookTotal)).toBeTruthy();
    expect(table.getByText("€1,111.00")).toBeTruthy();
    expect(table.getByText("€555.00")).toBeTruthy();
  });

  test("shows what is still owed in goods, and only where goods can move", async () => {
    reply("/inventory/order-book", {
      scope: "open",
      orders: [GOODS, SERVICES],
      totals: TOTALS,
      currencies: ["EUR"],
    });
    ui("/inventory/order-book");

    await screen.findByText("SO-2026-00004");
    // The invariant part of the hint, taken from the catalog rather than
    // written out here, so this test says the same thing in every language.
    const hintTail = strings.inventoryBookQtyHint("").trim();
    expect(hintTail).not.toBe("");

    // Four units still to go out on the order for goods…
    const rows = screen.getAllByRole("row");
    const goodsRow = rows.find((r) => within(r).queryByText("SO-2026-00004") !== null);
    expect(goodsRow?.textContent).toContain(hintTail);
    expect(within(goodsRow as HTMLElement).getByText(strings.inventoryBookQtyHint("4"))).toBeTruthy();

    // …and nothing of the sort on the one that is pure services, which is
    // outstanding €500.00 and no things at all. A "0 still to go out" on a
    // consultancy order would be a screen talking about the wrong thing.
    const servicesRow = rows.find((r) => within(r).queryByText("SO-2026-00005") !== null);
    expect(servicesRow).toBeTruthy();
    expect(servicesRow?.textContent).not.toContain(hintTail);
  });

  test("changing the scope asks the server again rather than filtering the page", async () => {
    reply("/inventory/order-book", {
      scope: "open",
      orders: [GOODS],
      totals: TOTALS,
      currencies: ["EUR"],
    });
    ui("/inventory/order-book");
    await screen.findByText("SO-2026-00004");
    expect(reads().some((u) => u.includes("scope=open"))).toBe(true);

    reply("/inventory/order-book", {
      scope: "all",
      orders: [GOODS, SERVICES],
      totals: TOTALS,
      currencies: ["EUR"],
    });
    fireEvent.change(screen.getByLabelText(strings.inventoryFilterScope), {
      target: { value: "all" },
    });

    // The server was asked for the wider scope — what counts as open is its
    // decision, not a predicate this screen keeps a second copy of.
    await waitFor(() => expect(reads().some((u) => u.includes("scope=all"))).toBe(true));
    expect(await screen.findByText("SO-2026-00005")).toBeTruthy();
  });

  test("a book in two currencies withholds the total and says why", async () => {
    reply("/inventory/order-book", {
      scope: "open",
      orders: [GOODS, { ...SERVICES, currency: "GBP" }],
      totals: TOTALS,
      currencies: ["EUR", "GBP"],
    });
    ui("/inventory/order-book");

    await screen.findByText("SO-2026-00004");
    // The reason is given in words, naming both currencies…
    expect(screen.getByText(/EUR/)).toBeTruthy();
    expect(screen.getByText(/GBP/)).toBeTruthy();
    // …and the total is not printed at all, rather than printed as a sum of
    // two different things.
    expect(screen.queryByText(strings.inventoryBookTotal)).toBeNull();
    expect(screen.queryByText("€1,111.00")).toBeNull();
  });

  test("a row leads to the order it stands for", async () => {
    reply("/inventory/order-book", {
      scope: "open",
      orders: [GOODS],
      totals: TOTALS,
      currencies: ["EUR"],
    });
    ui("/inventory/order-book");

    fireEvent.click(await screen.findByRole("button", { name: "SO-2026-00004" }));

    // The order's own screen loads its document — the book is a way in, and
    // the acts live on the document.
    await waitFor(() => expect(reads().some((u) => u.includes("/sales-orders/so-1"))).toBe(true));
  });

  test("an empty book says so in the words of the scope that was asked", async () => {
    reply("/inventory/order-book", {
      scope: "open",
      orders: [],
      totals: figures({}),
      currencies: [],
    });
    ui("/inventory/order-book");

    expect(await screen.findByText(strings.inventoryOrderBookEmptyTitle)).toBeTruthy();
    // No table at all, rather than a table of nothing with a zero total.
    expect(screen.queryByRole("table")).toBeNull();
  });
});
