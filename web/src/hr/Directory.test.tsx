// What the directory promises, proven against a recorded network: that it is
// every member's screen and asks no door for it, that one search box narrows
// both readings of the same people (and narrows the chart **with the managers
// above a match**), that the chart drawn is the server's tree and not a fold of
// `managerId` done here, that the people who have left are HR's read alone — and
// that a person's row leads to where they sit.
//
// Only the network and the session's three door answers are faked. The real
// router, the real module routes, the real client, the real filters and the real
// screens run.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import { HrModule } from "./HrModule";
import type { HrDirectoryEntry, HrOrgNode } from "./types";

interface Call {
  url: string;
  method: string;
}

const calls: Call[] = [];

/** What the session says about the three doors, set per test. Every one of them
 *  is shut by default: the point of this screen is that it works without one. */
let doors = { hr: false, books: false, admin: false };
/** Whether the people read refuses, so the failure path is a real refusal. */
let brokenDirectory = false;

function person(over: Partial<HrDirectoryEntry> & { id: string; name: string }): HrDirectoryEntry {
  return {
    givenName: "",
    familyName: "",
    preferredName: "",
    workEmail: "",
    workPhone: "",
    managerId: null,
    photoNodeId: null,
    jobTitle: "",
    team: "",
    startedOn: null,
    archived: false,
    ...over,
  };
}

const ADA = person({
  id: "emp-1",
  name: "Ada Kowalski",
  givenName: "Ada",
  familyName: "Kowalski",
  workEmail: "ada@example.test",
  workPhone: "+48 22 000 000",
  jobTitle: "Managing director",
  team: "Board",
  startedOn: "2021-03-01",
});

const BRAM = person({
  id: "emp-2",
  name: "Bram de Vries",
  givenName: "Bram",
  familyName: "de Vries",
  workEmail: "bram@example.test",
  jobTitle: "Engineer",
  team: "Platform",
  managerId: "emp-1",
  startedOn: "2024-09-16",
});

/** The reader themselves, and the person the deepest branch hangs from. */
const CHIARA = person({
  id: "emp-3",
  name: "Chiara Rossi",
  givenName: "Chiara",
  familyName: "Rossi",
  workEmail: "chiara@example.test",
  jobTitle: "Support",
  team: "Platform",
  managerId: "emp-2",
  startedOn: "2025-02-03",
});

/** Somebody who has left: HR's read only, and never in the chart. */
const DIEGO = person({
  id: "emp-4",
  name: "Diego Fernández",
  familyName: "Fernández",
  workEmail: "diego@example.test",
  jobTitle: "Engineer",
  team: "Platform",
  managerId: "emp-1",
  archived: true,
});

const ACTIVE = [ADA, BRAM, CHIARA];

/** The tree as the SERVER folds it — deliberately not the shape `managerId`
 *  would give: Chiara hangs under Bram, and a screen that rebuilt this from the
 *  rows would have to agree by accident. */
const CHART: HrOrgNode[] = [
  {
    id: ADA.id,
    name: ADA.name,
    jobTitle: ADA.jobTitle,
    team: ADA.team,
    managerId: null,
    reports: [
      {
        id: BRAM.id,
        name: BRAM.name,
        jobTitle: BRAM.jobTitle,
        team: BRAM.team,
        managerId: ADA.id,
        reports: [
          {
            id: CHIARA.id,
            name: CHIARA.name,
            jobTitle: CHIARA.jobTitle,
            team: CHIARA.team,
            managerId: BRAM.id,
            reports: [],
          },
        ],
      },
    ],
  },
];

const fakeFetch = vi.fn((url: string, init?: RequestInit) => {
  const method = init?.method ?? "GET";
  calls.push({ url, method });
  if (url.includes("/hr/employees") && brokenDirectory) {
    return Promise.resolve(
      new Response(JSON.stringify({ detail: "the directory is closed" }), {
        status: 403,
        headers: { "content-type": "application/problem+json" },
      }),
    );
  }
  const body = url.includes("/hr/employees")
    ? {
        employees: url.includes("includeArchived") ? [...ACTIVE, DIEGO] : ACTIVE,
        hr: doors.hr,
      }
    : url.includes("/hr/org")
      ? { chart: CHART }
      : url.includes("/hr/me")
        ? { employee: { id: CHIARA.id, name: CHIARA.name }, isHr: doors.hr }
        : url.includes("/hr/leave-requests")
          ? { requests: [] }
          : url.includes("/finance/expenses/pending")
            ? { expenses: [] }
            : url.includes("/projects/approvals")
              ? { weeks: [] }
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

/** The module as it is really mounted. */
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

function reads(fragment: string): Call[] {
  return calls.filter((c) => c.method === "GET" && c.url.includes(fragment));
}

/** How a row names its own person: the name cell, not the initials beside it and
 *  not a name in a "Reports to" cell, which is somebody else's row. */
function nameOf(r: HTMLElement): string {
  const cell = r.querySelector("[class*='personName']");
  return (cell?.textContent ?? "").replace(strings.hrYou, "").trim();
}

/** The row a person's name is in. */
function row(name: string): HTMLElement {
  const found = screen.getAllByRole("row").find((r) => nameOf(r) === name);
  if (found === undefined) throw new Error(`no row for ${name}`);
  return found;
}

function names(): string[] {
  return screen.getAllByRole("row").slice(1).map(nameOf);
}

beforeEach(() => {
  calls.length = 0;
  doors = { hr: false, books: false, admin: false };
  brokenDirectory = false;
  session = (url: string, init?: RequestInit) => fakeFetch(url, init);
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("the directory", () => {
  test("is where a member with no door lands, and holds the public fields", async () => {
    ui();
    await screen.findByText(strings.hrPeopleCount(3));

    // No door was needed for any of it, and no queue was read on the way.
    expect(screen.getByText(strings.hrTabDirectory)).toBeTruthy();
    expect(screen.queryByText(strings.hrTabHiring)).toBeNull();
    expect(reads("/hr/leave-requests")).toHaveLength(0);
    expect(reads("/finance/expenses/pending")).toHaveLength(0);

    // The row says how to reach them, who they report to and since when — the
    // manager's NAME, resolved from the same answer the rows came from.
    const bram = within(row(BRAM.name));
    expect(bram.getByText(BRAM.workEmail).getAttribute("href")).toBe(`mailto:${BRAM.workEmail}`);
    expect(bram.getByText(ADA.name)).toBeTruthy();
    expect(names()).toEqual([ADA.name, BRAM.name, CHIARA.name]);

    // Their own row is marked, from `/hr/me` and not from a guess at an address.
    expect(within(row(CHIARA.name)).getByText(strings.hrYou)).toBeTruthy();
    expect(screen.getAllByText(strings.hrYou)).toHaveLength(1);
  });

  test("one search box narrows the list, in any order and across fields", async () => {
    ui();
    await screen.findByText(strings.hrPeopleCount(3));
    const readsSoFar = reads("/hr/employees").length;

    fireEvent.change(screen.getByLabelText(strings.hrDirectorySearch), {
      target: { value: "platform vries" },
    });
    await waitFor(() => expect(names()).toEqual([BRAM.name]));
    expect(screen.getByText(strings.hrShowingOf(1, 3))).toBeTruthy();

    // What matched nothing says so, and offers the way out.
    fireEvent.change(screen.getByLabelText(strings.hrDirectorySearch), {
      target: { value: "warehouse" },
    });
    await screen.findByText(strings.hrNoMatchTitle("warehouse"));
    fireEvent.click(screen.getByText(strings.hrClearSearch));
    await waitFor(() => expect(names()).toHaveLength(3));

    // Every one of those narrowings was done here: not one of them asked the
    // server again.
    expect(reads("/hr/employees")).toHaveLength(readsSoFar);
  });

  test("the chart is the server's tree, and a search keeps the managers above a match", async () => {
    ui();
    await screen.findByText(strings.hrPeopleCount(3));

    fireEvent.click(screen.getByRole("button", { name: strings.hrViewOrg }));
    await waitFor(() => expect(reads("/hr/org")).toHaveLength(1));

    // Chiara sits under Bram, who sits under Ada — the shape the server served,
    // which is three levels deep and not the two `managerId` alone would give
    // if this screen folded it.
    const chiara = await screen.findByText(CHIARA.name);
    const branch = chiara.closest("li");
    expect(branch?.parentElement?.closest("li")?.textContent).toContain(BRAM.name);
    // Two people have somebody under them, and each says so.
    expect(screen.getAllByText(strings.hrReportsCount(1))).toHaveLength(2);

    // A search for the deepest person keeps the line above her: a chart that
    // dropped Ada and Bram would say something false about who Chiara works for.
    fireEvent.change(screen.getByLabelText(strings.hrDirectorySearch), {
      target: { value: "chiara" },
    });
    await waitFor(() => expect(screen.getByText(strings.hrShowingOf(3, 3))).toBeTruthy());
    expect(screen.getByText(ADA.name)).toBeTruthy();
    expect(screen.getByText(BRAM.name)).toBeTruthy();

    // And a search that no branch answers narrows the chart to nothing rather
    // than showing the whole company.
    fireEvent.change(screen.getByLabelText(strings.hrDirectorySearch), {
      target: { value: "warehouse" },
    });
    await screen.findByText(strings.hrNoMatchTitle("warehouse"));
  });

  test("a row leads to where that person sits, and the address carries it", async () => {
    ui();
    await screen.findByText(strings.hrPeopleCount(3));

    fireEvent.click(within(row(BRAM.name)).getByText(strings.hrShowInChart));

    // The chart, opened on that person — marked where they stand rather than
    // filtered down to them, so the people around them are the answer.
    const marked = await waitFor(() => {
      const found = document.querySelector('[aria-current="true"]');
      if (found === null) throw new Error("nobody marked");
      return found;
    });
    expect(marked.textContent).toContain(BRAM.name);
    expect(screen.getByText(ADA.name)).toBeTruthy();
  });

  test("the people who have left are HR's read alone", async () => {
    ui();
    await screen.findByText(strings.hrPeopleCount(3));
    // Not HR: no control for it, and nobody asked the server for leavers.
    expect(screen.queryByText(strings.hrIncludeLeavers)).toBeNull();
    expect(reads("includeArchived")).toHaveLength(0);
    cleanup();

    calls.length = 0;
    doors = { hr: true, books: false, admin: false };
    session = (url: string, init?: RequestInit) => fakeFetch(url, init);
    ui("/hr/directory");
    await screen.findByText(strings.hrPeopleCount(3));

    fireEvent.click(screen.getByLabelText(strings.hrIncludeLeavers));
    await waitFor(() => expect(reads("includeArchived")).toHaveLength(1));
    // They arrive said plainly, and with no way to look for them in the chart:
    // the chart is the people who are here.
    const gone = await screen.findByText(DIEGO.name);
    expect(within(row(DIEGO.name)).getByText(strings.hrLeft)).toBeTruthy();
    expect(within(gone.closest("tr") as HTMLElement).queryByText(strings.hrShowInChart)).toBeNull();
  });

  test("a refusal is the server's own sentence, not an empty company", async () => {
    brokenDirectory = true;
    ui();

    await screen.findByText("the directory is closed");
    // Nothing pretends the tenant has nobody in it.
    expect(screen.queryByText(strings.hrDirectoryEmptyTitle)).toBeNull();
  });
});
