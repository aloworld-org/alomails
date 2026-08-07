// What the VAT report screen promises, proven against a recorded network: that
// it asks the server for a stated period and never invents one, that every
// figure on screen is one the server sent (the browser adds nothing up), that
// two currencies stay two tables, and that the CSV is fetched through the
// authenticated client rather than linked — a plain link would download a 401.
//
// Only the network is fake. The real router, the real module routes, the real
// client and the real formatters all run.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import { BillingModule } from "./BillingModule";
import { previousQuarterOf, quarterOf } from "./period";
import type { VatReport } from "./types";

interface Call {
  url: string;
  method: string;
}

const calls: Call[] = [];

/** The quarter the screen opens on, computed the same way the view does. */
const THIS_QUARTER = quarterOf(new Date());

/** A period with two rates in euro and a second currency beside it — the two
 *  facts the screen must not blur: rates add up within a currency, currencies
 *  never add up to each other. */
const REPORT: VatReport = {
  from: THIS_QUARTER.from,
  to: THIS_QUARTER.to,
  currencies: [
    {
      currency: "EUR",
      invoiceCount: 5,
      creditNoteCount: 1,
      netCents: 127997,
      vatCents: 23880,
      grossCents: 151877,
      byRate: [
        { rateBp: 900, netCents: 25000, vatCents: 2250 },
        { rateBp: 2100, netCents: 102997, vatCents: 21630 },
      ],
    },
    {
      currency: "USD",
      invoiceCount: 1,
      creditNoteCount: 0,
      netCents: 20000,
      vatCents: 0,
      grossCents: 20000,
      byRate: [{ rateBp: 0, netCents: 20000, vatCents: 0 }],
    },
  ],
};

const CSV = "row,periodFrom,periodTo,currency,vatRatePercent,net,vat,gross,invoices,creditNotes\r\n";

const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
  const method = init?.method ?? "GET";
  calls.push({ url, method });
  if (url.includes("/billing/reports/vat.csv")) {
    return new Response(CSV, { status: 200, headers: { "content-type": "text/csv" } });
  }
  if (url.includes("/billing/reports/vat")) {
    return new Response(JSON.stringify({ report: REPORT }), {
      status: 200,
      headers: { "content-type": "application/json" },
    });
  }
  return new Response(JSON.stringify({}), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
});

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

function ui() {
  return render(
    <MemoryRouter initialEntries={["/billing/reports"]}>
      <DialogProvider>
        <Routes>
          <Route path="/billing/*" element={<BillingModule />} />
        </Routes>
      </DialogProvider>
    </MemoryRouter>,
  );
}

/** The URLs the report screen asked for, newest last. */
function reportCalls(): string[] {
  return calls.filter((c) => c.url.includes("/billing/reports/")).map((c) => c.url);
}

beforeEach(() => {
  calls.length = 0;
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("the VAT report", () => {
  test("asks for the current quarter and shows the server's figures per currency", async () => {
    ui();

    // The period is always stated on the wire — the screen never leaves it to
    // the server to guess which days it meant.
    await waitFor(() => expect(reportCalls().length).toBe(1));
    expect(reportCalls()[0]).toContain(`from=${THIS_QUARTER.from}&to=${THIS_QUARTER.to}`);

    // Two currencies, two tables: nothing on this screen adds a dollar to a
    // euro, because the API never offered a figure that did.
    const tables = await screen.findAllByRole("table");
    expect(tables.length).toBe(2);
    const euro = within(tables[0] as HTMLElement);
    // The rate rows are the server's, formatted but not recomputed.
    expect(euro.getByText("9%")).toBeTruthy();
    expect(euro.getByText("21%")).toBeTruthy();
    expect(euro.getByText("€250.00")).toBeTruthy();
    expect(euro.getByText("€22.50")).toBeTruthy();
    expect(euro.getByText("€1,029.97")).toBeTruthy();
    expect(euro.getByText("€216.30")).toBeTruthy();
    // And so are the totals: 1 279.97 net, 238.80 VAT, 1 518.77 gross — sent,
    // not summed here.
    expect(euro.getByText("€1,279.97")).toBeTruthy();
    expect(euro.getByText("€238.80")).toBeTruthy();
    expect(euro.getByText("€1,518.77")).toBeTruthy();
    expect(screen.getByText(strings.billingReportCounts(5, 1))).toBeTruthy();

    // The dollars stand alone: their own net and their own (zero) VAT, in
    // their own currency's symbol, in a table of their own.
    const dollars = within(tables[1] as HTMLElement);
    // Three times: the rate row, the total, and the gross.
    expect(dollars.getAllByText("$200.00").length).toBe(3);
    // Twice: no VAT at a zero rate, on the rate row and on the total.
    expect(dollars.getAllByText("$0.00").length).toBe(2);
    expect(dollars.queryByText(/€/)).toBeNull();
  });

  test("a chosen period is what is asked for, and what the screen says it shows", async () => {
    ui();
    await waitFor(() => expect(reportCalls().length).toBe(1));

    const last = previousQuarterOf(new Date());
    fireEvent.click(screen.getByText(strings.billingReportLastQuarter));

    await waitFor(() => expect(reportCalls().length).toBe(2));
    expect(reportCalls()[1]).toContain(`from=${last.from}&to=${last.to}`);
    // The boxes moved with the request: what is written above the figures is
    // what was asked for.
    const from = screen.getByLabelText(strings.billingReportFrom) as HTMLInputElement;
    const to = screen.getByLabelText(strings.billingReportTo) as HTMLInputElement;
    expect(from.value).toBe(last.from);
    expect(to.value).toBe(last.to);
  });

  test("a typed period is only requested when it is submitted", async () => {
    ui();
    await waitFor(() => expect(reportCalls().length).toBe(1));

    const from = screen.getByLabelText(strings.billingReportFrom);
    fireEvent.change(from, { target: { value: "2025-01-01" } });
    // A half-typed period is not a request: still exactly the one load.
    expect(reportCalls().length).toBe(1);

    fireEvent.click(screen.getByText(strings.billingReportShow));
    await waitFor(() => expect(reportCalls().length).toBe(2));
    expect(reportCalls()[1]).toContain(`from=2025-01-01&to=${THIS_QUARTER.to}`);
  });

  test("the CSV is fetched through the authenticated client, for the shown period", async () => {
    // Saving is a DOM act; what matters here is that the bytes came from the
    // API and are handed on under the period's own name.
    const clicked: { name: string; type: string }[] = [];
    const createObjectURL = vi.fn(() => "blob:vat");
    const revokeObjectURL = vi.fn();
    vi.stubGlobal("URL", { ...URL, createObjectURL, revokeObjectURL });
    const click = vi
      .spyOn(HTMLAnchorElement.prototype, "click")
      .mockImplementation(function (this: HTMLAnchorElement) {
        clicked.push({ name: this.download, type: this.href });
      });

    ui();
    await waitFor(() => expect(reportCalls().length).toBe(1));
    fireEvent.click(screen.getByText(strings.billingReportDownloadCsv));

    await waitFor(() => expect(clicked.length).toBe(1));
    const csvCall = reportCalls().find((url) => url.includes("vat.csv"));
    expect(csvCall).toContain(`from=${THIS_QUARTER.from}&to=${THIS_QUARTER.to}`);
    expect(clicked[0]?.name).toBe(`vat-${THIS_QUARTER.from}-to-${THIS_QUARTER.to}.csv`);
    expect(createObjectURL).toHaveBeenCalled();

    click.mockRestore();
    vi.unstubAllGlobals();
  });

  test("a period the server refuses is reported in the server's own words", async () => {
    fakeFetch.mockImplementationOnce(
      async () =>
        new Response(JSON.stringify({ detail: "to must be a date of the form YYYY-MM-DD" }), {
          status: 422,
          headers: { "content-type": "application/json" },
        }),
    );
    ui();

    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(screen.getByText("to must be a date of the form YYYY-MM-DD")).toBeTruthy();
  });
});
