// What the Accounts screen promises, proven against a recorded network.
//
// Four claims, each of them something a chart editor can silently get wrong
// about a set of books:
//
// - the chart is **grouped by kind**, and a first read that seeded it says so,
//   because twenty accounts nobody typed must not appear unexplained;
// - a **rename carries the role back unchanged** — on this table, dropping a
//   role quietly unhooks a posting rule and the next invoice stops booking;
// - **delete is offered only where it can succeed**: never on an account we
//   seeded, never on one that carries entries, and retiring is on the form;
// - a refusal — a code somebody else's account already has — is shown in the
//   **server's own words**, in the dialog, which stays open.
//
// Only the network is fake. The real router, the real module routes, the real
// client and the real i18n catalog all run.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import { FinanceModule } from "./FinanceModule";
import type { ChartAccount } from "./types";

interface Call {
  url: string;
  method: string;
  body: unknown;
}

const calls: Call[] = [];

/** An account we seeded: renameable, never deletable. */
const RECEIVABLES: ChartAccount = {
  id: "acc-ar",
  code: "1100",
  name: "Trade receivables",
  type: "asset",
  role: "ar",
  active: true,
  system: true,
  balanceCents: 250_000,
  debitCents: 250_000,
  creditCents: 0,
  postings: 4,
  createdAt: "2026-01-01T00:00:00Z",
  updatedAt: "2026-01-01T00:00:00Z",
};

/** The tenant's own line, never used. */
const HOSTING: ChartAccount = {
  id: "acc-hosting",
  code: "6110",
  name: "Hosting",
  type: "expense",
  role: null,
  active: true,
  system: false,
  balanceCents: 0,
  debitCents: 0,
  creditCents: 0,
  postings: 0,
  createdAt: "2026-01-01T00:00:00Z",
  updatedAt: "2026-01-01T00:00:00Z",
};

let accounts: ChartAccount[] = [];
let seeded = false;
/** What a write answers with. Replaced per test. */
let writeAnswer: () => Response = () => json({ account: HOSTING });

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
  if (url.includes("/finance/accounts")) {
    if (method === "GET") return json({ accounts, seeded, currency: "EUR" });
    if (method === "DELETE") return new Response(null, { status: 204 });
    return writeAnswer();
  }
  if (url.includes("/projects")) return json({ projects: [] });
  return json({});
});

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

vi.mock("../jmap", () => ({
  useJmapClient: () => ({ canWorkTheBooks: () => Promise.resolve(true) }),
}));

function ui() {
  return render(
    <MemoryRouter initialEntries={["/finance/accounts"]}>
      <DialogProvider>
        <Routes>
          <Route path="/finance/*" element={<FinanceModule />} />
        </Routes>
      </DialogProvider>
    </MemoryRouter>,
  );
}

/** Renders the tab and opens the editor of one account by its row's Edit. */
async function openEditor(name: string) {
  ui();
  const row = (await screen.findByText(name)).closest("tr");
  if (row === null) throw new Error(`no row for ${name}`);
  fireEvent.click(within(row as HTMLElement).getByText(strings.financeAccountEdit));
  return screen.findByRole("dialog");
}

beforeEach(() => {
  calls.length = 0;
  accounts = [RECEIVABLES, HOSTING];
  seeded = false;
  writeAnswer = () => json({ account: HOSTING });
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("reading the chart", () => {
  test("the accounts are grouped by what they hold", async () => {
    ui();
    expect(await screen.findByText(strings.financeAccountTypeAsset)).toBeTruthy();
    expect(screen.getByText(strings.financeAccountTypeExpense)).toBeTruthy();
    // A kind nothing is filed under is not an empty table with a heading.
    expect(screen.queryByText(strings.financeAccountTypeEquity)).toBeNull();
  });

  test("the read that seeded the chart says where the accounts came from", async () => {
    seeded = true;
    ui();
    expect(await screen.findByText(strings.financeChartSeeded)).toBeTruthy();
  });

  test("a chart nobody seeded says nothing about seeding", async () => {
    ui();
    await screen.findByText("Trade receivables");
    expect(screen.queryByText(strings.financeChartSeeded)).toBeNull();
  });

  test("a role is shown as the job it does, and the balance in the server's own currency", async () => {
    ui();
    const row = (await screen.findByText("Trade receivables")).closest("tr");
    expect(row?.textContent).toContain(strings.financeRoleAr);
    // 250_000 cents, read as money — never as cents, never re-derived.
    expect(row?.textContent).toContain("2,500.00");
    expect(row?.textContent).toContain("4");
  });

  test("the list is asked for over the period in the toolbar, in the reader's language", async () => {
    ui();
    await screen.findByText("Trade receivables");
    const read = calls.find((call) => call.url.includes("/finance/accounts"));
    expect(read?.url).toContain("lang=");
    expect(read?.url).toMatch(/from=\d{4}-01-01&to=\d{4}-12-31/);
  });
});

describe("editing an account", () => {
  test("a rename carries the role back unchanged", async () => {
    const dialog = await openEditor("Trade receivables");
    fireEvent.change(within(dialog).getByDisplayValue("Trade receivables"), {
      target: { value: "Debtors" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: strings.financeSave }));

    await waitFor(() => expect(calls.some((call) => call.method === "PATCH")).toBe(true));
    const patch = calls.find((call) => call.method === "PATCH");
    expect(patch?.url).toContain("/finance/accounts/acc-ar");
    expect(patch?.body).toEqual({
      code: "1100",
      name: "Debtors",
      type: "asset",
      // The whole point: a rename that dropped this would stop invoices booking.
      role: "ar",
      active: true,
    });
  });

  test("retiring an account is a field of the form, not a second door", async () => {
    const dialog = await openEditor("Hosting");
    fireEvent.change(within(dialog).getByDisplayValue(strings.financeAccountInUse), {
      target: { value: "no" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: strings.financeSave }));

    await waitFor(() => expect(calls.some((call) => call.method === "PATCH")).toBe(true));
    expect(calls.find((call) => call.method === "PATCH")?.body).toMatchObject({ active: false });
  });

  test("an account we seeded cannot be deleted, and says why", async () => {
    const dialog = await openEditor("Trade receivables");
    expect(within(dialog).queryByText(strings.financeAccountDelete)).toBeNull();
    expect(within(dialog).getByText(strings.financeAccountSystemNote)).toBeTruthy();
  });

  test("an account carrying entries cannot be deleted either", async () => {
    accounts = [{ ...HOSTING, postings: 2 }];
    const dialog = await openEditor("Hosting");
    expect(within(dialog).queryByText(strings.financeAccountDelete)).toBeNull();
  });

  test("the tenant's own unused account can be", async () => {
    const dialog = await openEditor("Hosting");
    fireEvent.click(within(dialog).getByRole("button", { name: strings.financeAccountDelete }));
    await waitFor(() => expect(calls.some((call) => call.method === "DELETE")).toBe(true));
    expect(calls.find((call) => call.method === "DELETE")?.url).toContain(
      "/finance/accounts/acc-hosting",
    );
  });

  test("a refusal is the server's own sentence, and the form stays open on it", async () => {
    writeAnswer = () =>
      json({ status: 409, detail: "an account with this code already exists" }, 409);
    const dialog = await openEditor("Hosting");
    fireEvent.change(within(dialog).getByDisplayValue("6110"), { target: { value: "1100" } });
    fireEvent.click(within(dialog).getByRole("button", { name: strings.financeSave }));

    expect(
      await within(dialog).findByText("an account with this code already exists"),
    ).toBeTruthy();
    // Still open, still holding what was typed: the person can fix it here.
    expect(within(dialog).getByDisplayValue("1100")).toBeTruthy();
  });
});

describe("adding an account", () => {
  test("nothing is sent until the kind is said", async () => {
    ui();
    fireEvent.click(await screen.findByRole("button", { name: strings.financeAccountAdd }));
    const dialog = await screen.findByRole("dialog");
    const inputs = within(dialog).getAllByRole("textbox");
    fireEvent.change(inputs[0] as HTMLElement, { target: { value: "6120" } });
    fireEvent.change(inputs[1] as HTMLElement, { target: { value: "Software" } });

    const add = within(dialog).getAllByRole("button", { name: strings.financeAccountAdd });
    const submit = add[add.length - 1] as HTMLButtonElement;
    expect(submit.disabled).toBe(true);

    fireEvent.change(within(dialog).getByDisplayValue(strings.financeAccountTypeUnset), {
      target: { value: "expense" },
    });
    fireEvent.click(submit);

    await waitFor(() => expect(calls.some((call) => call.method === "POST")).toBe(true));
    expect(calls.find((call) => call.method === "POST")?.body).toEqual({
      code: "6120",
      name: "Software",
      type: "expense",
      role: "",
    });
  });
});
