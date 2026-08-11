// What the time-off screens promise, proven against a recorded network: that a
// member with no door at all lands on their own leave and reads it with its
// working, that **the clock is the server's** and not the device's, that asking
// sends exactly what was chosen and nothing computed, that a decision travels
// the route that owns it, and that the absence calendar shows a name, a day and
// nothing else.
//
// Only the network and the session's three door answers are faked. The real
// router, the real module routes, the real client, the real pure functions and
// the real screens run.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import { dayLabel } from "./format";
import { HrModule } from "./HrModule";
import { daysLabel } from "./leave";
import type { HrDirectoryEntry, HrLeaveRequest } from "./types";

interface Call {
  url: string;
  method: string;
  body: unknown;
}

const calls: Call[] = [];

let doors = { hr: false, books: false, admin: false };
/** Whether this login has an employee record, or is the contractor-with-a-
 *  mailbox the balances route answers `409` for. */
let hasRecord = true;
/** Whether anybody reports to the caller — what makes them an approver. */
let manages = false;

/** The caller. */
const ME = "emp-3";

/** The server's own day, deliberately years away from any machine's clock: a
 *  screen that reached for `new Date()` would disagree with it visibly. */
const SERVER_TODAY = "2030-06-01";

function request(over: Partial<HrLeaveRequest> & { id: string }): HrLeaveRequest {
  return {
    employeeId: ME,
    employeeName: "Chiara Rossi",
    policyId: "pol-annual",
    policyName: "Annual leave",
    fromDay: "2030-07-01",
    toDay: "2030-07-05",
    status: "requested",
    note: "",
    costMinutes: 1920,
    workingDays: 4,
    holidayMinutes: 0,
    decidedBy: null,
    decidedAt: null,
    decisionNote: "",
    closedAt: null,
    createdAt: "2030-05-01T09:00:00Z",
    updatedAt: "2030-05-01T09:00:00Z",
    ...over,
  };
}

/** Waiting, and the caller's own: theirs to take back. */
const WAITING = request({ id: "req-wait", note: "A family thing" });

/** Booked, and — **on the server's calendar** — already begun. A device that
 *  thinks it is 2026 would call it a future absence and offer to cancel it. */
const BEGUN = request({
  id: "req-begun",
  status: "approved",
  fromDay: "2027-01-04",
  toDay: "2027-01-08",
  decidedBy: "u-1",
  decidedAt: "2026-12-01T10:00:00Z",
});

/** Booked and still ahead: the one that can be given back. */
const AHEAD = request({
  id: "req-ahead",
  status: "approved",
  fromDay: "2030-07-20",
  toDay: "2030-07-24",
  holidayMinutes: 480,
  decidedBy: "u-1",
  decidedAt: "2030-05-02T10:00:00Z",
});

/** Somebody else's, waiting for whoever is reading. */
const THEIRS = request({
  id: "req-theirs",
  employeeId: "emp-9",
  employeeName: "Bram de Vries",
  fromDay: "2030-08-03",
  toDay: "2030-08-07",
});

function policy(id: string, name: string, over: Record<string, unknown> = {}) {
  return {
    id,
    name,
    kind: "annual",
    entitlementMinutes: 12000,
    accrual: "yearly",
    leaveYearStartMonth: 1,
    leaveYearStartDay: 1,
    carryoverCapMinutes: 0,
    carryoverExpiresAfterMonths: null,
    allowNegative: false,
    requiresApproval: true,
    paid: true,
    archived: false,
    archivedAt: null,
    createdAt: "2030-01-01T00:00:00Z",
    updatedAt: "2030-01-01T00:00:00Z",
    ...over,
  };
}

const BALANCES = {
  employeeId: ME,
  on: SERVER_TODAY,
  balances: [
    {
      policy: policy("pol-annual", "Annual leave"),
      entitlementMinutes: 12000,
      carriedInMinutes: 0,
      accruedMinutes: 12000,
      takenMinutes: 2400,
      bookedMinutes: 1920,
      pendingMinutes: 1920,
      remainingMinutes: 6000,
      averageDayMinutes: 480,
      entitlementDaysTenths: 250,
      takenDaysTenths: 50,
      bookedDaysTenths: 40,
      pendingDaysTenths: 40,
      remainingDaysTenths: 125,
    },
    {
      policy: policy("pol-sick", "Sick leave", { kind: "sick", requiresApproval: false }),
      entitlementMinutes: 0,
      carriedInMinutes: 0,
      accruedMinutes: 0,
      takenMinutes: 0,
      bookedMinutes: 0,
      pendingMinutes: 0,
      remainingMinutes: 0,
      averageDayMinutes: 480,
      entitlementDaysTenths: 0,
      takenDaysTenths: 0,
      bookedDaysTenths: 0,
      pendingDaysTenths: 0,
      remainingDaysTenths: 0,
    },
  ],
};

const ABSENCES = [
  { day: "2026-08-10", people: [{ employeeId: "emp-9", name: "Bram de Vries" }] },
  {
    day: "2026-08-11",
    people: [
      { employeeId: "emp-9", name: "Bram de Vries" },
      { employeeId: "emp-1", name: "Ada Kowalski" },
    ],
  },
];

const HOLIDAYS = [{ date: "2026-08-15", key: "assumption", name: "Assumption" }];

function directory(): HrDirectoryEntry[] {
  const base = {
    givenName: "",
    familyName: "",
    preferredName: "",
    workEmail: "",
    workPhone: "",
    photoNodeId: null,
    jobTitle: "",
    team: "",
    startedOn: null,
    archived: false,
  };
  return [
    { ...base, id: ME, name: "Chiara Rossi", managerId: null },
    { ...base, id: "emp-9", name: "Bram de Vries", managerId: manages ? ME : null },
  ];
}

/** The rows a scope answers. `mine` is the caller's; `team` and `all` are the
 *  server naming the people, which is why the fake answers different sets
 *  rather than filtering one. */
function requestsFor(url: URL): HrLeaveRequest[] {
  const scope = url.searchParams.get("scope") ?? "mine";
  const wanted = (url.searchParams.get("status") ?? "").split(",").filter((s) => s !== "");
  const rows =
    scope === "mine"
      ? [WAITING, BEGUN, AHEAD]
      : scope === "team"
        ? [THEIRS]
        : [THEIRS, WAITING];
  return wanted.length === 0 ? rows : rows.filter((row) => wanted.includes(row.status));
}

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

const fakeFetch = vi.fn((raw: string, init?: RequestInit) => {
  const method = init?.method ?? "GET";
  const body = typeof init?.body === "string" ? (JSON.parse(init.body) as unknown) : null;
  calls.push({ url: raw, method, body });
  const url = new URL(raw, "http://localhost");
  const path = url.pathname;

  if (method === "POST" && /\/hr\/leave-requests\/[^/]+\/\w+$/.test(path)) {
    return Promise.resolve(json({ request: WAITING }));
  }
  if (method === "POST" && path === "/hr/leave-requests") {
    return Promise.resolve(json({ request: WAITING }));
  }
  if (path === "/hr/leave-requests") return Promise.resolve(json({ requests: requestsFor(url) }));
  if (path === "/hr/leave-balances") {
    return hasRecord
      ? Promise.resolve(json(BALANCES))
      : Promise.resolve(
          new Response(
            JSON.stringify({ detail: "this login is not linked to an employee record" }),
            { status: 409, headers: { "content-type": "application/problem+json" } },
          ),
        );
  }
  if (path === "/hr/absences") return Promise.resolve(json({ days: ABSENCES }));
  if (path === "/hr/holidays") return Promise.resolve(json({ holidays: HOLIDAYS }));
  if (path === "/hr/employees") return Promise.resolve(json({ employees: directory(), hr: doors.hr }));
  if (path === "/hr/me") {
    return Promise.resolve(
      json({ employee: hasRecord ? { id: ME, name: "Chiara Rossi" } : null, isHr: doors.hr }),
    );
  }
  if (path === "/finance/expenses/pending") return Promise.resolve(json({ expenses: [] }));
  if (path === "/projects/approvals") return Promise.resolve(json({ weeks: [] }));
  return Promise.resolve(json({ openings: [], chart: [] }));
});

/** A fresh function per test: the doors are cached against the session's own
 *  fetch (`queues.ts`), so a new session is what makes a new answer. */
let session = fakeFetch as (url: string, init?: RequestInit) => Promise<Response>;

vi.mock("../auth", () => ({
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

function ui(path = "/hr") {
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

function seen(fragment: string, method = "GET"): Call[] {
  return calls.filter((c) => c.method === method && c.url.includes(fragment));
}

/** The row a request's dates are written in — found by the dates themselves,
 *  formatted the way the screen formats them. */
function rowOf(request: HrLeaveRequest): HTMLElement {
  const when = strings.hrLeaveBetween(dayLabel(request.fromDay), dayLabel(request.toDay));
  const found = screen.getAllByRole("row").find((row) => (row.textContent ?? "").includes(when));
  if (found === undefined) throw new Error(`no row for ${request.id}`);
  return found;
}

beforeEach(() => {
  calls.length = 0;
  doors = { hr: false, books: false, admin: false };
  hasRecord = true;
  manages = false;
  session = (url: string, init?: RequestInit) => fakeFetch(url, init);
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("my leave", () => {
  test("is where a member with no door lands, and the balance comes with its working", async () => {
    ui();
    await screen.findByText(daysLabel(125));
    await screen.findByRole("table");

    // No door was asked for any of it, and the tab is drawn for everybody.
    expect(screen.getByText(strings.hrTabLeave)).toBeTruthy();
    expect(screen.queryByText(strings.hrTabHiring)).toBeNull();
    // One card per policy the tenant runs, each saying what it is — and the
    // figure belongs to the policy it is drawn under.
    const card = screen.getByText(daysLabel(125)).closest("div") as HTMLElement;
    expect(within(card).getByText("Annual leave")).toBeTruthy();
    expect(screen.getByText(strings.hrNotDecided)).toBeTruthy();
    // The working, and the day the server folded to — not this machine's.
    expect(screen.getByText(strings.hrFactOf(strings.hrBalanceTaken, daysLabel(50)))).toBeTruthy();
    expect(screen.getByText(strings.hrBalanceAsOf(dayLabel(SERVER_TODAY)))).toBeTruthy();
    // The cost of a request is the server's figure, shown as it was sent.
    expect(screen.getAllByText(strings.hrWorkingDays(4)).length).toBeGreaterThan(0);
    // Somebody with nobody under them is asked no second question.
    expect(screen.queryByText(strings.hrScopeTeam)).toBeNull();
  });

  test("the clock is the server's: leave that has begun cannot be given back", async () => {
    ui("/hr/leave");
    await screen.findByRole("table");

    // Ahead of the server's day → it can be given back, and the reason a week
    // costs less than five days is said on the row.
    const ahead = within(rowOf(AHEAD));
    expect(ahead.getByText(strings.hrCancelLeave)).toBeTruthy();
    expect(ahead.getByText(strings.hrHolidaysInside)).toBeTruthy();

    // Behind it → no button at all. Every machine that runs this test believes
    // it is years before 2027-01-04, so a screen reading its own clock would
    // offer to cancel this one.
    expect(within(rowOf(BEGUN)).queryByText(strings.hrCancelLeave)).toBeNull();

    // And what is offered actually travels the route that owns it, after which
    // the record is re-read rather than edited here.
    const before = seen("/hr/leave-requests").length;
    fireEvent.click(ahead.getByText(strings.hrCancelLeave));
    await waitFor(() => expect(seen("/cancel", "POST")).toHaveLength(1));
    await waitFor(() => expect(seen("/hr/leave-requests").length).toBeGreaterThan(before));
  });

  test("only the person who asked is offered the way to take it back", async () => {
    ui("/hr/leave");
    await screen.findByRole("table");

    expect(within(rowOf(WAITING)).getByText(strings.hrWithdraw)).toBeTruthy();
    expect(within(rowOf(AHEAD)).queryByText(strings.hrWithdraw)).toBeNull();

    fireEvent.click(within(rowOf(WAITING)).getByText(strings.hrWithdraw));
    await waitFor(() => expect(seen("/withdraw", "POST")).toHaveLength(1));
  });

  test("asking sends the dates chosen, and shows who else is off then", async () => {
    ui("/hr/leave");
    await screen.findByText(daysLabel(125));
    fireEvent.click(screen.getByText(strings.hrAskForLeave));

    // The picker offers the policies the balance read named — no vocabulary of
    // this screen's own.
    const dialog = within(await screen.findByRole("dialog"));
    expect(dialog.getByRole("option", { name: "Annual leave" })).toBeTruthy();
    expect(dialog.getByRole("option", { name: "Sick leave" })).toBeTruthy();

    // Both days chosen through the real picker, one after the other.
    for (const index of [0, 1]) {
      const triggers = dialog.getAllByRole("button", { expanded: false });
      fireEvent.click(triggers[index] as HTMLElement);
      fireEvent.click(await screen.findByText(strings.datePickerToday));
    }

    // The window is asked about as chosen, and the answer is named people —
    // each of them once, however many days they are off.
    await waitFor(() => expect(seen("/hr/absences").length).toBeGreaterThan(0));
    const asked = new URL(
      (seen("/hr/absences").at(-1) as Call).url,
      "http://localhost",
    ).searchParams;
    expect(asked.get("from")).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    expect(asked.get("to")).toBe(asked.get("from"));
    await screen.findByText("Bram de Vries, Ada Kowalski");

    fireEvent.change(dialog.getByRole("textbox"), { target: { value: "Wedding" } });
    fireEvent.click(dialog.getByText(strings.hrAskSubmit));

    await waitFor(() => expect(seen("/hr/leave-requests", "POST")).toHaveLength(1));
    const sent = (seen("/hr/leave-requests", "POST")[0] as Call).body as Record<string, string>;
    expect(sent.policyId).toBe("pol-annual");
    expect(sent.note).toBe("Wedding");
    expect(sent.fromDay).toBe(asked.get("from"));
    expect(sent.toDay).toBe(asked.get("to"));
    // No number of days was sent: what it costs is the server's to work out.
    expect(Object.keys(sent).sort()).toEqual(["fromDay", "note", "policyId", "toDay"]);
  });

  test("a login the tenant has no record for is told so, and offered nothing to press", async () => {
    hasRecord = false;
    ui("/hr/leave");

    await screen.findByText("this login is not linked to an employee record");
    // No balance invented, and no button whose only outcome is that sentence
    // again.
    expect(screen.queryByText(strings.hrAskForLeave)).toBeNull();
    expect(screen.queryByText(strings.hrBalanceLeft)).toBeNull();
  });
});

describe("deciding somebody else's", () => {
  test("a manager asks the team question, and the row they came for is marked", async () => {
    manages = true;
    ui("/hr/leave?scope=team&request=req-theirs");

    await screen.findByText(THEIRS.employeeName);
    // The scope is a question put to the server, not a filter applied here.
    expect(seen("scope=team")).toHaveLength(1);
    expect(seen("scope=mine")).toHaveLength(0);
    // A manager who is not HR is not offered everybody's leave.
    expect(screen.getByText(strings.hrScopeTeam)).toBeTruthy();
    expect(screen.queryByText(strings.hrScopeEveryone)).toBeNull();

    const row = rowOf(THEIRS);
    expect(row.getAttribute("aria-current")).toBe("true");

    fireEvent.click(within(row).getByText(strings.hrApprove));
    await waitFor(() => expect(seen("/approve", "POST")).toHaveLength(1));
    // Re-read, never edited here: the row leaves because the server said so.
    await waitFor(() => expect(seen("scope=team").length).toBeGreaterThan(1));
  });

  test("HR sees everybody's, and is offered no decision on their own", async () => {
    doors = { hr: true, books: false, admin: false };
    ui("/hr/leave?scope=all");

    await screen.findByText(THEIRS.employeeName);
    expect(screen.getByText(strings.hrScopeEveryone)).toBeTruthy();
    // Somebody else's: theirs to decide.
    expect(within(rowOf(THEIRS)).getByText(strings.hrApprove)).toBeTruthy();
    // Their own, in the same list: leave is never approved by the person taking
    // it, so the control is not drawn to be refused.
    expect(within(rowOf(WAITING)).queryByText(strings.hrApprove)).toBeNull();
    expect(within(rowOf(WAITING)).getByText(strings.hrWithdraw)).toBeTruthy();
  });
});

describe("who's away", () => {
  test("draws the served days and the tenant's holidays, and pages by month", async () => {
    ui("/hr/away?month=2026-08");

    // The whole grid is asked about, spill days included.
    await waitFor(() => expect(seen("/hr/absences")).toHaveLength(1));
    const asked = new URL((seen("/hr/absences")[0] as Call).url, "http://localhost").searchParams;
    expect(asked.get("from")).toBe("2026-07-27");
    expect(asked.get("to")).toBe("2026-09-06");

    // A name and a day, and nothing else: two people away, counted once each.
    await screen.findByText(strings.hrAwayThisMonth(2));
    expect(screen.getAllByText("Bram de Vries")).toHaveLength(2);
    expect(screen.getByText("Ada Kowalski")).toBeTruthy();
    // The company's own calendar, behind the same grid.
    expect(screen.getByText("Assumption")).toBeTruthy();

    fireEvent.click(screen.getByLabelText(strings.hrNextMonth));
    await waitFor(() => expect(seen("/hr/absences")).toHaveLength(2));
    const next = new URL((seen("/hr/absences")[1] as Call).url, "http://localhost").searchParams;
    expect(next.get("from")).toBe("2026-08-31");
    expect(next.get("to")).toBe("2026-10-11");
  });
});
