// What the bank screens promise, proven against a recorded network.
//
// The four claims worth a test, all of them things a screen can silently get
// wrong about somebody's money:
//
// - the import is **two steps**, and the first one calls the preview route,
//   which writes nothing;
// - a `422` is rendered as the **report**, naming the rows, not as a sentence
//   that leaves a person with nothing to do;
// - a confirmed match sends the **line's own amount**, never a number this
//   screen worked out, so the server's comparison is meaningful;
// - the evidence tokens the server sends become sentences in the reader's
//   language, and a token this client does not know is dropped rather than
//   printed raw.
//
// Only the network is fake. The real router, the real module routes, the real
// client, and the real i18n catalog all run.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import { FinanceModule } from "./FinanceModule";
import type { BankImportReport, BankLine, BankStatement, BankSuggestions } from "./types";

interface Call {
  url: string;
  method: string;
  body: unknown;
}

const calls: Call[] = [];

const STATEMENT: BankStatement = {
  id: "st-1",
  accountIban: "DE02120300000000202051",
  currency: "EUR",
  source: "camt",
  statementRef: "2026/08",
  openingBalanceCents: 100_000,
  closingBalanceCents: 213_700,
  fromDate: "2026-08-01",
  toDate: "2026-08-31",
  importedBy: "u-1",
  importedAt: "2026-09-01T08:00:00Z",
  lineCount: 3,
};

/** One transaction nobody has attributed yet. */
const LINE: BankLine = {
  id: "bl-1",
  statementId: "st-1",
  lineNo: 1,
  bookedOn: "2026-08-14",
  valueOn: "2026-08-14",
  amountCents: 130_700,
  currency: "EUR",
  counterpartyName: "Acme GmbH",
  counterpartyIban: "DE89370400440532013000",
  remittance: "Rechnung INV-2026-00007",
  bankRef: "NONREF",
  status: "unmatched",
  ignoredReason: null,
  createdAt: "2026-09-01T08:00:00Z",
};

/** The reading of a CSV whose date column the server could not use. */
const REFUSED_REPORT: BankImportReport = {
  committed: false,
  source: "csv",
  encoding: "windows-1252",
  delimiter: ";",
  columns: ["Buchungstag", "Betrag", "Verwendungszweck"],
  mapping: {
    date: "Buchungstag",
    valueDate: null,
    amount: "Betrag",
    debit: null,
    credit: null,
    sign: null,
    currencyColumn: null,
    counterparty: null,
    iban: null,
    remittance: "Verwendungszweck",
    reference: null,
  },
  dates: "dmy",
  decimal: "comma",
  totalRows: 4,
  counts: { lines: 2, skipped: 0, errors: 1, staged: null, duplicates: null, unbooked: 0 },
  account: "DE02120300000000202051",
  currency: "EUR",
  period: { from: "2026-08-01", to: "2026-08-31" },
  sample: [],
  sampleTruncated: false,
  skippedLines: [],
  errors: [{ line: 3, rule: "the row's booking date is missing" }],
  statement: null,
};

/** The same file read cleanly. */
const GOOD_REPORT: BankImportReport = {
  ...REFUSED_REPORT,
  counts: { lines: 3, skipped: 0, errors: 0, staged: null, duplicates: null, unbooked: 0 },
  errors: [],
  sample: [
    {
      line: 2,
      bookedOn: "2026-08-14",
      valueOn: "2026-08-14",
      amountCents: 130_700,
      currency: "EUR",
      counterpartyName: "Acme GmbH",
      counterpartyIban: null,
      remittance: "Rechnung INV-2026-00007",
      bankRef: null,
    },
  ],
};

/** What each read answers with, unless a test says otherwise. */
let statements: BankStatement[] = [];
let suggestions: BankSuggestions = { lines: [], numbersCapped: false, ledgerCapped: false };
let matched: BankLine[] = [];
let ignored: BankLine[] = [];
/** What the two import doors do. Replaced per test. */
let previewAnswer: () => Response = () => json({ import: GOOD_REPORT });
let importAnswer: () => Response = () => json({ import: { ...GOOD_REPORT, committed: true } });

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
  if (url.includes("/finance/imports/bank/preview")) return previewAnswer();
  if (url.includes("/finance/imports/bank")) return importAnswer();
  if (method !== "GET") return json({ line: LINE, match: { id: "m-1" } });
  if (url.includes("/finance/bank/statements")) return json({ statements });
  if (url.includes("/finance/bank/suggestions")) return json({ suggestions });
  if (url.includes("/finance/bank/lines")) {
    return json({ lines: url.includes("status=matched") ? matched : ignored });
  }
  if (url.includes("/billing/invoices")) return json({ invoices: [] });
  if (url.includes("/projects")) return json({ projects: [] });
  return json({});
});

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

// The session flag behind the three bookkeeper tabs.
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

/** A file as a browser hands one over. */
function statementFile(): File {
  return new File(["Buchungstag;Betrag\n14.08.2026;1.307,00\n"], "august.csv", {
    type: "text/csv",
  });
}

/** Puts a file into the dialog's file input, which is how the flow starts. */
function chooseFile(dialog: HTMLElement) {
  const input = dialog.querySelector('input[type="file"]');
  if (input === null) throw new Error("the import dialog has no file input");
  fireEvent.change(input, { target: { files: [statementFile()] } });
}

beforeEach(() => {
  calls.length = 0;
  statements = [];
  suggestions = { lines: [], numbersCapped: false, ledgerCapped: false };
  matched = [];
  ignored = [];
  previewAnswer = () => json({ import: GOOD_REPORT });
  importAnswer = () => json({ import: { ...GOOD_REPORT, committed: true } });
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("importing a statement", () => {
  test("an empty tab onboards instead of showing an empty table", async () => {
    ui("/finance/bank");
    expect(await screen.findByText(strings.financeBankEmptyTitle)).toBeTruthy();
  });

  test("an imported statement is listed by what it covers, in the money it is in", async () => {
    statements = [STATEMENT];
    ui("/finance/bank");
    const row = (await screen.findByText(/DE02120300000000202051/)).closest("tr");
    expect(row).toBeTruthy();
    // The closing balance the bank stated, read as money rather than cents.
    expect(row?.textContent).toContain("2,137.00");
    expect(row?.textContent).toContain(strings.financeBankSourceCamt);
  });

  test("the first step is the preview, which writes nothing", async () => {
    ui("/finance/bank");
    fireEvent.click(await screen.findByRole("button", { name: strings.financeBankImportStatement }));
    const dialog = await screen.findByRole("dialog");
    chooseFile(dialog);

    fireEvent.click(within(dialog).getByRole("button", { name: strings.financeBankCheckFile }));
    await waitFor(() =>
      expect(calls.some((c) => c.url.includes("/finance/imports/bank/preview"))).toBe(true),
    );
    // Nothing was staged: the commit door was not called at all.
    expect(calls.some((c) => /\/finance\/imports\/bank(\?|$)/.test(c.url))).toBe(false);
    // …and the reading is now on screen, with the server's own column guess.
    expect(await within(dialog).findByText(strings.financeBankSampleTitle)).toBeTruthy();
  });

  test("a refused file shows the rows that broke, not a sentence on its own", async () => {
    importAnswer = () =>
      json(
        {
          type: "about:blank",
          status: 422,
          detail: "some rows of this file cannot be read; nothing was imported",
          import: REFUSED_REPORT,
        },
        422,
      );
    ui("/finance/bank");
    fireEvent.click(await screen.findByRole("button", { name: strings.financeBankImportStatement }));
    const dialog = await screen.findByRole("dialog");
    chooseFile(dialog);
    fireEvent.click(within(dialog).getByRole("button", { name: strings.financeBankCheckFile }));
    fireEvent.click(await within(dialog).findByRole("button", { name: strings.financeBankImport }));

    // The server's own sentence, and the row it is about.
    expect(
      await within(dialog).findByText(
        "some rows of this file cannot be read; nothing was imported",
      ),
    ).toBeTruthy();
    expect(within(dialog).getByText(strings.financeBankRowAt(3))).toBeTruthy();
    expect(within(dialog).getByText(/the row's booking date is missing/)).toBeTruthy();
  });

  test("correcting the mapping sends a person back to the preview, never straight to the commit", async () => {
    ui("/finance/bank");
    fireEvent.click(await screen.findByRole("button", { name: strings.financeBankImportStatement }));
    const dialog = await screen.findByRole("dialog");
    chooseFile(dialog);
    fireEvent.click(within(dialog).getByRole("button", { name: strings.financeBankCheckFile }));
    await within(dialog).findByRole("button", { name: strings.financeBankImport });

    fireEvent.change(within(dialog).getByLabelText(strings.financeBankColRemittance), {
      target: { value: "Betrag" },
    });
    // The primary action is the dry run again, and the screen says why.
    expect(within(dialog).getByText(strings.financeBankStale)).toBeTruthy();
    expect(within(dialog).getByRole("button", { name: strings.financeBankCheckFile })).toBeTruthy();
  });
});

describe("matching what arrived", () => {
  test("a certain guess is confirmed with the line's own amount", async () => {
    suggestions = {
      lines: [
        {
          line: LINE,
          exact: [
            {
              invoiceId: "inv-7",
              number: "INV-2026-00007",
              amountCents: 130_700,
              daysAfterIssue: 12,
            },
          ],
          likely: [],
        },
      ],
      numbersCapped: false,
      ledgerCapped: false,
    };
    ui("/finance/reconcile");
    fireEvent.click(await screen.findByRole("button", { name: strings.financeBankThisOne }));

    await waitFor(() =>
      expect(calls.some((c) => c.url.includes("/finance/bank/lines/bl-1/match"))).toBe(true),
    );
    const write = calls.find((c) => c.url.includes("/match"));
    // The line's own amount, verbatim — the number the server compares with
    // what the bank said. Nothing here worked out a figure.
    expect(write?.body).toMatchObject({ invoiceId: "inv-7", amountCents: 130_700 });
  });

  test("the evidence behind a guess is read as sentences, and an unknown token is dropped", async () => {
    suggestions = {
      lines: [
        {
          line: LINE,
          exact: [],
          likely: [
            {
              invoiceId: "inv-9",
              number: "INV-2026-00009",
              amountCents: 200_000,
              outstandingCents: 69_300,
              customerId: "cus-1",
              daysAfterIssue: 3,
              score: 72,
              evidence: [
                { kind: "numberQuoted" },
                { kind: "partPayment", remainingCents: 69_300 },
                // A stage this client has not learned yet.
                { kind: "somethingNewer" } as never,
              ],
              ruleId: "rule-4",
            },
          ],
        },
      ],
      numbersCapped: false,
      ledgerCapped: false,
    };
    ui("/finance/reconcile");

    const why = await screen.findByText(new RegExp(strings.financeBankWhyNumberQuoted));
    // The remainder is read as money, in the line's own currency — never as
    // the raw count of cents the wire carries.
    expect(why.textContent).toContain("693");
    expect(why.textContent).not.toContain("69300");
    // The unknown token contributed nothing at all — not even its name.
    expect(why.textContent).not.toContain("somethingNewer");

    // …and taking the suggestion sends the rule that proposed it, so the
    // server can count the hit.
    fireEvent.click(screen.getByRole("button", { name: strings.financeBankThisOne }));
    await waitFor(() => expect(calls.some((c) => c.url.includes("/match"))).toBe(true));
    expect(calls.find((c) => c.url.includes("/match"))?.body).toMatchObject({ ruleId: "rule-4" });
  });

  test("a line nobody can guess still offers the manual pick and the way out", async () => {
    suggestions = {
      lines: [{ line: LINE, exact: [], likely: [] }],
      numbersCapped: false,
      ledgerCapped: false,
    };
    ui("/finance/reconcile");
    expect(await screen.findByText(strings.financeBankNoGuess)).toBeTruthy();
    expect(screen.getByRole("button", { name: strings.financeBankPickInvoice })).toBeTruthy();
    expect(screen.getByRole("button", { name: strings.financeBankNotOurs })).toBeTruthy();
  });

  test("a matched line carries its undo, and taking it back hits the route that reverses it", async () => {
    matched = [{ ...LINE, id: "bl-2", status: "matched" }];
    ui("/finance/reconcile");
    fireEvent.click(await screen.findByRole("button", { name: strings.financeBankUndoMatch }));
    await waitFor(() =>
      expect(calls.some((c) => c.url.includes("/finance/bank/lines/bl-2/unmatch"))).toBe(true),
    );
  });

  test("a capped read says so, so a short list is never read as an empty pile", async () => {
    suggestions = { lines: [], numbersCapped: false, ledgerCapped: true };
    ui("/finance/reconcile");
    expect(await screen.findByText(strings.financeBankCapped)).toBeTruthy();
  });
});
