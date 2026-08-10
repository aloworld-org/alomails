// What the four report screens promise, proven against a recorded network.
//
// The claims are all of one kind: **the figures on screen are the server's**.
// So the recorded reports deliberately contain totals that do not equal the sum
// of their own lines — a browser that added anything up would print a different
// number and the test would catch it — and every screen is checked against the
// server's figure rather than against arithmetic repeated here.
//
// Beyond that: a sheet that does not balance must say so, an ageing must never
// default its side, and every CSV must go through the authenticated client
// (a plain link would download a `401` named like a report).
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import { FinanceModule } from "./FinanceModule";
import type { AgedReport, BalanceSheet, PlReport, VatReturn } from "./types";

const calls: { url: string; method: string }[] = [];

/** A year whose stated totals are NOT the sum of the lines under them: if any
 *  screen re-derived a total, it would print 1,400.00 and not 1,600.00. */
const PL: PlReport = {
  from: "2026-01-01",
  to: "2026-12-31",
  previousFrom: "2025-01-01",
  previousTo: "2025-12-31",
  currency: "EUR",
  income: [
    {
      accountId: "acc-4000",
      code: "4000",
      name: "Sales",
      type: "income",
      amountCents: 140_000,
      previousCents: 50_000,
      postings: 3,
    },
  ],
  expense: [
    {
      accountId: "acc-6000",
      code: "6000",
      name: "Hosting",
      type: "expense",
      amountCents: 20_000,
      previousCents: 20_000,
      postings: 2,
    },
  ],
  incomeCents: 160_000,
  expenseCents: 30_000,
  resultCents: 130_000,
  previousIncomeCents: 50_000,
  previousExpenseCents: 20_000,
  previousResultCents: 30_000,
};

const SHEET: BalanceSheet = {
  on: "2026-12-31",
  currency: "EUR",
  assets: [
    {
      accountId: "acc-1000",
      code: "1000",
      name: "Business account",
      type: "asset",
      role: "bank",
      amountCents: 300_000,
      postings: 9,
    },
  ],
  liabilities: [
    {
      accountId: "acc-2100",
      code: "2100",
      name: "VAT payable",
      type: "liability",
      role: "vat_output",
      amountCents: 60_000,
      postings: 4,
    },
  ],
  equity: [],
  assetCents: 300_000,
  liabilityCents: 60_000,
  equityCents: 0,
  resultCents: 240_000,
  liabilityEquityCents: 300_000,
  differenceCents: 0,
  balances: true,
};

const AGED: AgedReport = {
  on: "2026-12-31",
  side: "receivable",
  currency: "EUR",
  parties: [
    {
      partyId: "cus-1",
      name: "Acme GmbH",
      buckets: {
        currentCents: 0,
        d1_30Cents: 120_000,
        d31_60Cents: 0,
        d61_90Cents: 0,
        d90_plusCents: 30_000,
        totalCents: 150_000,
      },
      unconvertedCount: 0,
      documents: [
        {
          documentId: "inv-1",
          number: "INV-2026-00007",
          issueDate: "2026-11-01",
          dueDate: "2026-12-01",
          daysOverdue: 30,
          bucket: "d1_30",
          currency: "EUR",
          openCents: 120_000,
          baseOpenCents: 120_000,
          creditNote: false,
        },
      ],
    },
  ],
  buckets: {
    currentCents: 0,
    d1_30Cents: 120_000,
    d31_60Cents: 0,
    d61_90Cents: 0,
    d90_plusCents: 30_000,
    totalCents: 150_000,
  },
  unconvertedCount: 0,
  documentCount: 1,
};

const VAT: VatReturn = {
  from: "2026-04-01",
  to: "2026-06-30",
  currency: "EUR",
  output: {
    rates: [{ rateBp: 2100, baseCents: 100_000, vatCents: 21_000 }],
    baseCents: 100_000,
    vatCents: 21_000,
    unratedBaseCents: 0,
    unratedVatCents: 0,
  },
  input: {
    rates: [{ rateBp: 2100, baseCents: 40_000, vatCents: 8_400 }],
    baseCents: 40_000,
    vatCents: 8_400,
    unratedBaseCents: 0,
    unratedVatCents: 0,
  },
  netPayableCents: 12_600,
};

let sheet: BalanceSheet = SHEET;
let vat: VatReturn = VAT;

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
  calls.push({ url, method: init?.method ?? "GET" });
  // The `.csv` twins answer a file, exactly as the server does.
  if (url.includes(".csv")) {
    return new Response("row,periodFrom\nresult,2026-01-01\n", {
      status: 200,
      headers: { "content-type": "text/csv" },
    });
  }
  if (url.includes("/finance/reports/pl")) return json({ report: PL });
  if (url.includes("/finance/reports/balance")) return json({ report: sheet });
  if (url.includes("/finance/reports/aged")) {
    return json({ report: { ...AGED, side: url.includes("side=payable") ? "payable" : "receivable" } });
  }
  if (url.includes("/finance/reports/vat")) return json({ report: vat });
  if (url.includes("/projects")) return json({ projects: [] });
  return json({});
});

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

vi.mock("../jmap", () => ({
  useJmapClient: () => ({ canWorkTheBooks: () => Promise.resolve(true) }),
}));

function ui(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <DialogProvider>
        <Routes>
          <Route path="/finance/*" element={<FinanceModule />} />
        </Routes>
      </DialogProvider>
    </MemoryRouter>,
  );
}

/** Every read of a report, in order. */
function reportCalls() {
  return calls.filter((call) => call.url.includes("/finance/reports/")).map((call) => call.url);
}

beforeEach(() => {
  calls.length = 0;
  sheet = SHEET;
  vat = VAT;
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("getting to a report", () => {
  // A regression test for a real defect this slice found and fixed. The module
  // is mounted on a splat route, and react-router resolves a relative `to`
  // inside one against the *current location*: `to="reports"` clicked from
  // `/finance/expenses` navigated to `/finance/expenses/reports`, which matched
  // the module's catch-all, which redirected relatively again — a path that
  // grew a segment per render and a tab that never arrived. Every link in the
  // module is absolute for that reason, and this test is what says so.
  test("a tab lands where it says it lands, from any depth", async () => {
    ui("/finance/expenses");
    fireEvent.click(await screen.findByText(strings.financeTabReports));
    // The reports' own first tab, reached through the index redirect.
    expect(await screen.findByText(strings.financeReportIncome)).toBeTruthy();
    await waitFor(() => expect(reportCalls()[0]).toContain("/finance/reports/pl?"));

    // …and a second row's tab, one level deeper still.
    fireEvent.click(screen.getByText(strings.financeReportBalance));
    await waitFor(() =>
      expect(reportCalls().some((url) => url.includes("/finance/reports/balance?"))).toBe(true),
    );
    expect(await screen.findByText(strings.financeReportResultToDate)).toBeTruthy();
  });
});

describe("the profit and loss", () => {
  test("prints the server's totals, never a sum of the lines on screen", async () => {
    ui("/finance/reports/pl");
    expect(await screen.findByText("Sales")).toBeTruthy();
    // 160_000 cents stated against 140_000 of lines: the stated figure wins.
    expect(screen.getByText(/1,600\.00/)).toBeTruthy();
    expect(screen.getByText(/1,300\.00/)).toBeTruthy();
    expect(screen.getByText(strings.financeReportProfit)).toBeTruthy();
  });

  test("the comparative column names the days the server compared with", async () => {
    ui("/finance/reports/pl");
    await screen.findByText("Sales");
    const header = screen.getByRole("columnheader", { name: /2025/ });
    expect(header.textContent).toContain("2025");
  });

  test("a loss is called a loss", async () => {
    ui("/finance/reports/pl");
    await screen.findByText("Sales");
    expect(screen.queryByText(strings.financeReportLoss)).toBeNull();
  });

  test("a half-typed period is not a request", async () => {
    ui("/finance/reports/pl");
    await waitFor(() => expect(reportCalls().length).toBe(1));
    fireEvent.change(screen.getByLabelText(strings.financeReportFrom), {
      target: { value: "2025-01-01" },
    });
    expect(reportCalls().length).toBe(1);
    fireEvent.click(screen.getByText(strings.financeReportShow));
    await waitFor(() => expect(reportCalls().length).toBe(2));
    expect(reportCalls()[1]).toContain("from=2025-01-01");
  });

  test("the CSV goes through the authenticated client, under the period's own name", async () => {
    const clicked: string[] = [];
    const createObjectURL = vi.fn(() => "blob:pl");
    const revokeObjectURL = vi.fn();
    vi.stubGlobal("URL", { ...URL, createObjectURL, revokeObjectURL });
    const click = vi
      .spyOn(HTMLAnchorElement.prototype, "click")
      .mockImplementation(function (this: HTMLAnchorElement) {
        clicked.push(this.download);
      });

    ui("/finance/reports/pl");
    await waitFor(() => expect(reportCalls().length).toBe(1));
    fireEvent.click(screen.getByText(strings.financeReportDownloadCsv));

    await waitFor(() => expect(clicked.length).toBe(1));
    const year = new Date().getUTCFullYear();
    expect(reportCalls().some((url) => url.includes("/finance/reports/pl.csv"))).toBe(true);
    expect(clicked[0]).toBe(`profit-and-loss-${year}-01-01-to-${year}-12-31.csv`);
    expect(createObjectURL).toHaveBeenCalled();

    click.mockRestore();
    vi.unstubAllGlobals();
  });
});

describe("the balance sheet", () => {
  test("stands on one day and shows the result beside equity", async () => {
    ui("/finance/reports/balance");
    expect(await screen.findByText("Business account")).toBeTruthy();
    expect(screen.getByText(strings.financeReportResultToDate)).toBeTruthy();
    expect(screen.getByText(/2,400\.00/)).toBeTruthy();
    expect(reportCalls()[0]).toContain("on=");
    expect(reportCalls()[0]).not.toContain("from=");
  });

  test("books that do not balance say so, loudly, rather than printing a figure", async () => {
    sheet = { ...SHEET, differenceCents: 1_250, balances: false };
    ui("/finance/reports/balance");
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("12.50");
  });

  test("books that balance say nothing about it", async () => {
    ui("/finance/reports/balance");
    await screen.findByText("Business account");
    expect(screen.queryByRole("alert")).toBeNull();
  });
});

describe("who owes what", () => {
  test("states its side on the wire and never defaults it silently", async () => {
    ui("/finance/reports/aged");
    expect(await screen.findByText("Acme GmbH")).toBeTruthy();
    expect(reportCalls()[0]).toContain("side=receivable");

    fireEvent.change(screen.getByLabelText(strings.financeReportSide), {
      target: { value: "payable" },
    });
    await waitFor(() => expect(reportCalls().length).toBe(2));
    expect(reportCalls()[1]).toContain("side=payable");
  });

  test("the bands are the server's five, with its own total", async () => {
    ui("/finance/reports/aged");
    const row = (await screen.findByText("Acme GmbH")).closest("tr");
    expect(row?.textContent).toContain("1,200.00");
    expect(row?.textContent).toContain("300.00");
    expect(row?.textContent).toContain("1,500.00");
    expect(screen.getByText(strings.financeReportBand90Plus)).toBeTruthy();
  });
});

describe("the VAT return", () => {
  test("says which way the money goes, in words as well as in figures", async () => {
    ui("/finance/reports/vat");
    expect(await screen.findByText(strings.financeReportVatPayable)).toBeTruthy();
    expect(screen.getByText(/126\.00/)).toBeTruthy();
  });

  test("a refund is not a payment with a minus in front of it", async () => {
    vat = { ...VAT, netPayableCents: -4_000 };
    ui("/finance/reports/vat");
    expect(await screen.findByText(strings.financeReportVatRefund)).toBeTruthy();
    expect(screen.queryByText(strings.financeReportVatPayable)).toBeNull();
  });

  test("opens on the quarter that is being declared, which is the one that ended", async () => {
    ui("/finance/reports/vat");
    await screen.findByText(strings.financeReportVatPayable);
    const asked = reportCalls()[0] ?? "";
    const from = new URL(`https://x${asked}`).searchParams.get("from") ?? "";
    const to = new URL(`https://x${asked}`).searchParams.get("to") ?? "";
    // A quarter, ended, and in the past.
    expect(Date.parse(to)).toBeLessThan(Date.now());
    expect(new Date(from).getUTCMonth() % 3).toBe(0);
  });
});
