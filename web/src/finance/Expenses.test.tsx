// What the expenses screens promise, proven against a recorded network: that a
// typed amount reaches the API as integer cents and an empty currency box is
// sent as no currency at all (so the *server's* default decides), that a claim
// the server froze offers no way to edit it, that handing one in and taking it
// back hit the routes they say they do, and that the approver's two queues are
// two reads with two different decisions on them.
//
// Only the network is fake. The real router, the real module routes, the real
// client and Billing's real money parser all run.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import { FinanceModule } from "./FinanceModule";
import type { Expense, PendingExpense } from "./types";

interface Call {
  url: string;
  method: string;
  body: unknown;
}

const calls: Call[] = [];

/** A draft claim: the traveller's own money, still theirs to change. */
const DRAFT: Expense = {
  id: "exp-1",
  spentOn: "2026-08-03",
  categoryId: null,
  merchant: "Bahn",
  description: "Berlin → München",
  grossCents: 11_900,
  vatCents: 1_900,
  netCents: 10_000,
  vatRateBp: 1_900,
  currency: "EUR",
  method: "personal",
  projectId: null,
  receiptNodeId: null,
  status: "draft",
  editable: true,
  owesTheEmployee: true,
  submittedAt: null,
  decidedBy: null,
  decidedAt: null,
  decisionNote: "",
  reimbursedOn: null,
  proposedCategoryId: null,
  proposedAt: null,
  proposedReason: null,
  proposalDeclinedAt: null,
  createdAt: "2026-08-03T10:00:00Z",
  updatedAt: "2026-08-03T10:00:00Z",
};

/** The same claim once it is in somebody's queue: frozen, by the server's own
 *  `editable`, which is the only thing the screen reads. */
const SUBMITTED: Expense = {
  ...DRAFT,
  id: "exp-2",
  status: "submitted",
  editable: false,
  submittedAt: "2026-08-04T08:00:00Z",
};

/** A refused claim, with the sentence its claimant is meant to read. */
const REFUSED: Expense = {
  ...DRAFT,
  id: "exp-3",
  status: "rejected",
  editable: true,
  submittedAt: "2026-08-04T08:00:00Z",
  decidedAt: "2026-08-05T09:00:00Z",
  decidedBy: "u-boss",
  decisionNote: "the receipt is missing",
};

/** A claim waiting on an approver's desk. */
const WAITING: PendingExpense = {
  ...SUBMITTED,
  id: "exp-9",
  userId: "u-2",
  userEmail: "traveller@acme.test",
  categoryName: "Travel",
};

/** A claim the company approved and still owes the person for. */
const OWED: PendingExpense = {
  ...WAITING,
  id: "exp-10",
  status: "approved",
  editable: false,
  decidedAt: "2026-08-06T09:00:00Z",
  decidedBy: "u-boss",
};

/** What each GET answers with, unless a test says otherwise. */
let claims: Expense[] = [];
let waiting: PendingExpense[] = [];
let owed: PendingExpense[] = [];
/** Whether the session says this user may work the books. */
let approver = true;

const fakeFetch = vi.fn(async (url: string, init?: RequestInit) => {
  const method = init?.method ?? "GET";
  calls.push({
    url,
    method,
    body: typeof init?.body === "string" ? JSON.parse(init.body) : undefined,
  });
  return new Response(JSON.stringify(answer(url, method)), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
});

function answer(url: string, method: string): unknown {
  if (method !== "GET") return { expense: DRAFT };
  if (url.includes("/finance/expenses/pending")) return { expenses: waiting };
  if (url.includes("/finance/expenses/reimbursable")) return { expenses: owed };
  if (url.includes("/finance/expenses")) return { expenses: claims };
  if (url.includes("/projects")) return { projects: [{ id: "prj-1", name: "Acme rebuild" }] };
  return {};
}

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: fakeFetch }),
}));

// The session flag behind the Approvals tab. The real client would read it from
// `/.well-known/jmap`; what matters to these screens is only whether it is true.
vi.mock("../jmap", () => ({
  useJmapClient: () => ({ canWorkTheBooks: () => Promise.resolve(approver) }),
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

/** The last write the client made, if it made one. */
function lastWrite(): Call | undefined {
  return calls.filter((c) => c.method !== "GET").at(-1);
}

beforeEach(() => {
  calls.length = 0;
  claims = [];
  waiting = [];
  owed = [];
  approver = true;
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("my claims", () => {
  test("an empty period onboards instead of showing an empty table", async () => {
    ui("/finance/expenses");
    expect(await screen.findByText(strings.financeExpensesEmptyTitle)).toBeTruthy();
    // …and the period asked for is a real one, at both ends.
    const read = calls.find((c) => c.url.includes("/finance/expenses?"));
    expect(read).toBeTruthy();
    const query = new URL(read?.url ?? "", "http://x").searchParams;
    expect(query.get("from")).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    expect(query.get("to")).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });

  test("a typed amount reaches the API as integer cents, with no currency invented", async () => {
    ui("/finance/expenses");
    fireEvent.click(await screen.findByRole("button", { name: strings.financeNewClaim }));

    const dialog = await screen.findByRole("dialog");
    fireEvent.change(within(dialog).getByLabelText(strings.financeMerchant), {
      target: { value: "Bahn" },
    });
    // A comma decimal: a Dutch user with an English UI still types Dutch numbers.
    fireEvent.change(within(dialog).getByLabelText(strings.financeGross), {
      target: { value: "119,00" },
    });
    fireEvent.change(within(dialog).getByLabelText(strings.financeVat), {
      target: { value: "19" },
    });
    fireEvent.change(within(dialog).getByLabelText(strings.financeVatRate), {
      target: { value: "19" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: strings.financeSave }));

    await waitFor(() => expect(lastWrite()?.method).toBe("POST"));
    const write = lastWrite();
    expect(write?.url).toContain("/finance/expenses");
    expect(write?.body).toMatchObject({
      merchant: "Bahn",
      grossCents: 11_900,
      vatCents: 1_900,
      vatRateBp: 1_900,
      method: "personal",
      projectId: null,
    });
    // The box was left empty, so the server's own default decides.
    expect(write?.body).not.toHaveProperty("currency");
  });

  test("an amount that is not a number is refused before it can be sent", async () => {
    ui("/finance/expenses");
    fireEvent.click(await screen.findByRole("button", { name: strings.financeNewClaim }));
    const dialog = await screen.findByRole("dialog");
    fireEvent.change(within(dialog).getByLabelText(strings.financeGross), {
      target: { value: "about twenty" },
    });
    expect(within(dialog).getByText(strings.financeAmountInvalid)).toBeTruthy();
    const save = within(dialog).getByRole("button", { name: strings.financeSave });
    fireEvent.click(save);
    expect(lastWrite()).toBeUndefined();
  });

  test("a frozen claim offers to be taken back, and never to be edited", async () => {
    claims = [SUBMITTED];
    ui("/finance/expenses");
    expect(await screen.findByText(strings.financeStatusSubmitted)).toBeTruthy();
    expect(screen.queryByRole("button", { name: strings.financeEdit })).toBeNull();
    expect(screen.queryByRole("button", { name: strings.financeSubmit })).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: strings.financeWithdraw }));
    await waitFor(() => expect(lastWrite()?.url).toContain("/finance/expenses/exp-2/withdraw"));
    expect(lastWrite()?.method).toBe("POST");
  });

  test("a draft is handed in by its own verb, on its own claim", async () => {
    claims = [DRAFT];
    ui("/finance/expenses");
    fireEvent.click(await screen.findByRole("button", { name: strings.financeSubmit }));
    await waitFor(() => expect(lastWrite()?.url).toContain("/finance/expenses/exp-1/submit"));
  });

  test("a refusal shows the sentence the approver wrote, and stays editable", async () => {
    claims = [REFUSED];
    ui("/finance/expenses");
    expect(await screen.findByText("the receipt is missing")).toBeTruthy();
    expect(screen.getByRole("button", { name: strings.financeEdit })).toBeTruthy();
    expect(screen.getByRole("button", { name: strings.financeSubmit })).toBeTruthy();
  });

  test("the status filter is sent to the server rather than applied in the browser", async () => {
    claims = [DRAFT];
    ui("/finance/expenses");
    await screen.findByText("Bahn");
    fireEvent.change(screen.getByLabelText(strings.financeStatus), {
      target: { value: "approved" },
    });
    await waitFor(() => {
      const last = calls.filter((c) => c.url.includes("/finance/expenses?")).at(-1);
      expect(new URL(last?.url ?? "", "http://x").searchParams.get("status")).toBe("approved");
    });
  });
});

describe("the approver's queues", () => {
  test("the tab is not drawn at all for somebody who may not work the books", async () => {
    approver = false;
    ui("/finance/expenses");
    await screen.findByText(strings.financeExpensesEmptyTitle);
    expect(screen.queryByText(strings.financeTabApprovals)).toBeNull();
    // …and nothing on the claimant's screen asked the approver's routes.
    expect(calls.some((c) => c.url.includes("/pending"))).toBe(false);
  });

  test("waiting claims are approved and refused by name, and the two queues are two reads", async () => {
    waiting = [WAITING];
    ui("/finance/approvals");
    expect(await screen.findByText("traveller@acme.test")).toBeTruthy();
    expect(calls.some((c) => c.url.includes("/finance/expenses/pending"))).toBe(true);
    expect(calls.some((c) => c.url.includes("/finance/expenses/reimbursable"))).toBe(true);
    expect(screen.getByText(strings.financeOwedEmptyTitle)).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: strings.financeApprove }));
    await waitFor(() => expect(lastWrite()?.url).toContain("/finance/expenses/exp-9/approve"));
  });

  test("paying a claim back sends the day the money moved", async () => {
    owed = [OWED];
    ui("/finance/approvals");
    fireEvent.click(await screen.findByRole("button", { name: strings.financeMarkPaidBack }));

    const dialog = await screen.findByRole("dialog");
    fireEvent.change(within(dialog).getByLabelText(strings.financeReimbursedOn), {
      target: { value: "2026-08-09" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: strings.financeMarkPaidBack }));

    await waitFor(() => expect(lastWrite()?.url).toContain("/finance/expenses/exp-10/reimburse"));
    expect(lastWrite()?.body).toEqual({ reimbursedOn: "2026-08-09" });
  });
});
