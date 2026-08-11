// What the one approvals inbox promises, proven against a recorded network:
// that three modules' queues arrive as one list ordered by who has waited
// longest, that every decision travels the route of the module that owns the
// record (and never a fourth endpoint of the inbox's own), that sending
// something back asks for a sentence first, that a queue which fails is named
// rather than shown as empty — and that somebody with no door reads nothing at
// all.
//
// Only the network and the session's three door answers are faked. The real
// router, the real module routes, the real queues, the real merge and the real
// screen run: the point of the item is that one screen agrees with three APIs,
// and a test against stubs could not tell.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import type { PendingExpense } from "../finance/types";
import type { PendingWeek } from "../projects/types";
import { HrModule } from "./HrModule";
import type { HrLeaveRequest } from "./types";

interface Call {
  url: string;
  method: string;
  body: unknown;
}

const calls: Call[] = [];

/** What the session says about the three doors, set per test. */
let doors = { hr: true, books: true, admin: true };
/** Which queue reads fail, so the inbox's failure path is a real refusal and
 *  not a mocked flag. */
let brokenExpenses = false;

/** The oldest wait: a claim handed in on the 1st. */
const CLAIM: PendingExpense = {
  id: "exp-1",
  userId: "u-2",
  userEmail: "bas@example.test",
  categoryName: "Travel",
  spentOn: "2026-07-30",
  categoryId: "cat-1",
  merchant: "Bahn",
  description: "Return to Berlin",
  grossCents: 11_900,
  vatCents: 1900,
  netCents: 10_000,
  vatRateBp: 1900,
  currency: "EUR",
  method: "personal",
  projectId: null,
  receiptNodeId: null,
  status: "submitted",
  editable: false,
  owesTheEmployee: true,
  submittedAt: "2026-08-01T09:00:00Z",
  decidedBy: null,
  decidedAt: null,
  decisionNote: "",
  reimbursedOn: null,
  proposedCategoryId: null,
  proposedAt: null,
  proposedReason: null,
  proposalDeclinedAt: null,
  createdAt: "2026-08-01T09:00:00Z",
  updatedAt: "2026-08-01T09:00:00Z",
};

/** The middle wait: leave asked for on the 3rd. */
const LEAVE: HrLeaveRequest = {
  id: "lr-1",
  employeeId: "emp-1",
  employeeName: "Amara Diallo",
  policyId: "pol-1",
  policyName: "Annual leave",
  fromDay: "2026-09-07",
  toDay: "2026-09-11",
  status: "requested",
  note: "Family visit",
  costMinutes: 2400,
  workingDays: 5,
  holidayMinutes: 0,
  decidedBy: null,
  decidedAt: null,
  decisionNote: "",
  closedAt: null,
  createdAt: "2026-08-03T08:00:00Z",
  updatedAt: "2026-08-03T08:00:00Z",
};

/** The newest wait: a week handed in on the 5th. */
const WEEK: PendingWeek = {
  id: "wk-1",
  userId: "u-3",
  userEmail: "chidi@example.test",
  weekStart: "2026-07-27",
  weekEnd: "2026-08-02",
  status: "submitted",
  locked: true,
  submittedAt: "2026-08-05T07:00:00Z",
  decidedBy: null,
  decidedAt: null,
  decisionNote: "",
  minutes: 2250,
  billableMinutes: 1800,
  createdAt: "2026-08-05T07:00:00Z",
  updatedAt: "2026-08-05T07:00:00Z",
};

const fakeFetch = vi.fn((url: string, init?: RequestInit) => {
  const method = init?.method ?? "GET";
  calls.push({
    url,
    method,
    body: typeof init?.body === "string" ? JSON.parse(init.body) : undefined,
  });
  if (url.includes("/finance/expenses/pending") && brokenExpenses) {
    return Promise.resolve(
      new Response(JSON.stringify({ detail: "the books are not yours" }), {
        status: 403,
        headers: { "content-type": "application/problem+json" },
      }),
    );
  }
  const body =
    method !== "GET"
      ? { request: LEAVE, expense: CLAIM, week: WEEK }
      : url.includes("/hr/leave-requests")
        ? { requests: [LEAVE], scope: "team" }
        : url.includes("/finance/expenses/pending")
          ? { expenses: [CLAIM] }
          : url.includes("/projects/approvals")
            ? { weeks: [WEEK] }
            : url.includes("/hr/me")
              ? { employee: { id: "emp-9", name: "The manager" }, isHr: false }
              : url.includes("/hr/employees")
                ? { employees: [{ id: "emp-1", managerId: "emp-9", archived: false }] }
                : { openings: [] };
  return Promise.resolve(
    new Response(JSON.stringify(body), {
      status: 200,
      headers: { "content-type": "application/json" },
    }),
  );
});

/** A fresh function per test: the doors are cached against the session's own
 *  fetch (`queues.ts`), so a new session is what makes a new answer. */
let session = fakeFetch as (url: string, init?: RequestInit) => Promise<Response>;

vi.mock("../auth", () => ({
  // `session` itself, never a wrapper: a new function per render would
  // re-create every module client and loop the effects keyed on them.
  useAuth: () => ({ authorizedFetch: session, identity: { sub: "u-1", email: "", name: "" } }),
}));

vi.mock("../jmap", () => ({
  useJmapClient: () => ({
    canWorkHr: () => Promise.resolve(doors.hr),
    canWorkTheBooks: () => Promise.resolve(doors.books),
    isAdmin: () => Promise.resolve(doors.admin),
    driveDownload: () => Promise.resolve(new Blob([])),
  }),
}));

vi.mock("../drive", () => ({ saveBlob: vi.fn() }));

/** The module as it is really mounted, opened on the inbox. */
function ui(path = "/hr/approvals") {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <DialogProvider>
        <Routes>
          <Route path="/hr/*" element={<HrModule />} />
        </Routes>
      </DialogProvider>
    </MemoryRouter>,
  );
}

function writes(): Call[] {
  return calls.filter((c) => c.method !== "GET");
}

function reads(fragment: string): Call[] {
  return calls.filter((c) => c.method === "GET" && c.url.includes(fragment));
}

/** The row a person's name is in. */
function row(person: string): HTMLElement {
  const cell = screen.getByText(person);
  const found = cell.closest("tr");
  if (found === null) throw new Error(`no row for ${person}`);
  return found;
}

beforeEach(() => {
  calls.length = 0;
  doors = { hr: true, books: true, admin: true };
  brokenExpenses = false;
  session = (url: string, init?: RequestInit) => fakeFetch(url, init);
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("the one approvals inbox", () => {
  test("shows three modules' queues as one list, longest wait first", async () => {
    ui();
    await screen.findByText(LEAVE.employeeName);

    const people = screen
      .getAllByRole("row")
      .slice(1)
      .map((r) => r.querySelector("td")?.textContent);
    // The claim was handed in on the 1st, the leave on the 3rd, the week on the
    // 5th — and that, not the order the three queues answered in, is the order.
    expect(people).toEqual([CLAIM.userEmail, LEAVE.employeeName, WEEK.userEmail]);

    // The count is the sum, and each kind is named beside it.
    expect(screen.getByText(strings.hrWaitingCount(3))).toBeTruthy();
    expect(screen.getByText(strings.hrCountOf(strings.hrQueueLeave, 1))).toBeTruthy();
    expect(screen.getByText(strings.hrCountOf(strings.hrQueueExpense, 1))).toBeTruthy();
    expect(screen.getByText(strings.hrCountOf(strings.hrQueueTimesheet, 1))).toBeTruthy();

    // Each row is spoken about in its own module's words: the days of leave are
    // the server's fold, the money is Finance's formatting of its cents.
    expect(within(row(LEAVE.employeeName)).getByText(strings.hrWorkingDays(5))).toBeTruthy();
    expect(within(row(LEAVE.employeeName)).getByText(LEAVE.note)).toBeTruthy();
  });

  test("every decision travels the route of the module that owns the record", async () => {
    ui();
    await screen.findByText(LEAVE.employeeName);

    fireEvent.click(within(row(LEAVE.employeeName)).getByText(strings.hrApprove));
    await waitFor(() => expect(writes()).toHaveLength(1));
    expect(writes()[0]?.url).toContain(`/hr/leave-requests/${LEAVE.id}/approve`);

    fireEvent.click(within(row(CLAIM.userEmail)).getByText(strings.hrApprove));
    await waitFor(() => expect(writes()).toHaveLength(2));
    expect(writes()[1]?.url).toContain(`/finance/expenses/${CLAIM.id}/approve`);

    fireEvent.click(within(row(WEEK.userEmail)).getByText(strings.hrApprove));
    await waitFor(() => expect(writes()).toHaveLength(3));
    expect(writes()[2]?.url).toContain(`/projects/approvals/${WEEK.id}/approve`);

    // Three decisions, three modules, and not one request to an endpoint of the
    // inbox's own: the doors stay where the data is.
    expect(writes().every((c) => c.method === "POST")).toBe(true);
    expect(calls.some((c) => c.url.includes("/approvals?"))).toBe(false);
  });

  test("sending something back asks for a sentence, and carries it", async () => {
    ui();
    await screen.findByText(WEEK.userEmail);

    fireEvent.click(within(row(WEEK.userEmail)).getByText(strings.hrSendBack));
    const asked = await screen.findByRole("dialog", { name: strings.hrSendBackTitle });
    // Nothing has been sent while the question is on screen.
    expect(writes()).toHaveLength(0);
    expect(asked.textContent).toContain(WEEK.userEmail);

    fireEvent.change(within(asked).getByRole("textbox"), {
      target: { value: "Thursday is on the wrong project" },
    });
    fireEvent.click(within(asked).getByRole("button", { name: strings.hrSendBack }));

    await waitFor(() => expect(writes()).toHaveLength(1));
    const sent = writes()[0] as Call;
    expect(sent.url).toContain(`/projects/approvals/${WEEK.id}/reject`);
    expect(sent.body).toEqual({ note: "Thursday is on the wrong project" });
  });

  test("a cancelled question decides nothing", async () => {
    ui();
    await screen.findByText(WEEK.userEmail);

    fireEvent.click(within(row(WEEK.userEmail)).getByText(strings.hrSendBack));
    const asked = await screen.findByRole("dialog", { name: strings.hrSendBackTitle });
    fireEvent.click(within(asked).getByRole("button", { name: strings.hrCancel }));

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(writes()).toHaveLength(0);
  });

  test("a queue that refuses is named, and does not empty the inbox", async () => {
    brokenExpenses = true;
    ui();
    await screen.findByText(LEAVE.employeeName);

    // The two that answered are still decidable…
    expect(screen.getByText(WEEK.userEmail)).toBeTruthy();
    expect(screen.queryByText(CLAIM.userEmail)).toBeNull();
    // …and the list says it is short rather than reading as "nothing waiting".
    expect(
      screen.getByText(strings.hrApprovalsQueueFailed(strings.hrQueueExpense)),
    ).toBeTruthy();
    // The count is of what is actually known, and the kind that failed is not
    // counted as zero.
    expect(screen.getByText(strings.hrWaitingCount(2))).toBeTruthy();
    expect(screen.queryByText(strings.hrCountOf(strings.hrQueueExpense, 0))).toBeNull();
  });

  test("somebody with no door is shown no inbox and reads no queue", async () => {
    doors = { hr: false, books: false, admin: false };
    // Nobody reports to them: the directory holds one person, managed by
    // somebody else. It is a whole row because the screen this test opens draws
    // these rows — the directory, deliberately, since a member's own leave (the
    // landing since B6.08b) reads a queue of its own and this test is about the
    // ones it must not read.
    const colleague = {
      id: "emp-1",
      name: "Someone Else",
      givenName: "Someone",
      familyName: "Else",
      preferredName: "",
      workEmail: "else@example.test",
      workPhone: "",
      managerId: "emp-2",
      photoNodeId: null,
      jobTitle: "Engineer",
      team: "Platform",
      startedOn: null,
      archived: false,
    };
    const empty = vi.fn((url: string, init?: RequestInit) => {
      if (url.includes("/hr/employees")) {
        calls.push({ url, method: "GET", body: undefined });
        return Promise.resolve(
          new Response(JSON.stringify({ employees: [colleague], hr: false }), {
            status: 200,
            headers: { "content-type": "application/json" },
          }),
        );
      }
      return fakeFetch(url, init);
    });
    session = empty;

    ui("/hr/directory");
    await screen.findByText(colleague.name);
    expect(screen.queryByText(strings.hrTabApprovals)).toBeNull();
    expect(reads("/hr/leave-requests")).toHaveLength(0);
    expect(reads("/finance/expenses/pending")).toHaveLength(0);
    expect(reads("/projects/approvals")).toHaveLength(0);
  });

  test("HR asks for the tenant's leave; a manager asks for their own team", async () => {
    ui();
    await waitFor(() => expect(reads("/hr/leave-requests")).toHaveLength(1));
    expect(reads("/hr/leave-requests")[0]?.url).toContain("scope=all");
    // HR's answer needs no org read: the wider door already answered.
    expect(reads("/hr/employees")).toHaveLength(0);
    cleanup();

    calls.length = 0;
    doors = { hr: false, books: false, admin: false };
    session = (url: string, init?: RequestInit) => fakeFetch(url, init);
    ui();
    await waitFor(() => expect(reads("/hr/leave-requests")).toHaveLength(1));
    expect(reads("/hr/leave-requests")[0]?.url).toContain("scope=team");
    expect(reads("/hr/leave-requests")[0]?.url).toContain("status=requested");
    // And with no other door, the manager's inbox holds leave alone.
    expect(reads("/finance/expenses/pending")).toHaveLength(0);
    expect(reads("/projects/approvals")).toHaveLength(0);
  });
});
