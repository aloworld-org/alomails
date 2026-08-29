// What the hiring screen promises, proven against a recorded network: that the
// columns are the stages the API served (and not a list this app holds), that a
// drag is exactly one move request, that recording somebody posts to the round
// that is on screen, that erasing a candidate asks first and then really
// deletes — and that a member who is not HR is never shown a hiring board at
// all.
//
// Only the network, the session's door answer and the file-saving are faked.
// The real router, the real module routes, the real client, the real board, the
// real drawer and the real forms all run: the point of the item is that these
// screens agree with the API, and a test against stubs could not tell.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import { HrModule } from "./HrModule";
import type { HrApplicant, HrOpening } from "./types";

interface Call {
  url: string;
  method: string;
  body: unknown;
}

const calls: Call[] = [];
/** Whether the session's door says this user may work HR. */
let isHr = true;

const OPENING: HrOpening = {
  id: "op-1",
  title: "Backend engineer",
  team: "Platform",
  location: "Rotterdam",
  employmentKind: "permanent",
  status: "open",
  openedOn: "2026-08-01",
  closedOn: null,
  applicants: 2,
  createdBy: "u-1",
  createdAt: "2026-08-01T09:00:00Z",
  updatedAt: "2026-08-01T09:00:00Z",
};

function person(id: string, name: string, stage: string, over = false): HrApplicant {
  return {
    id,
    openingId: OPENING.id,
    name,
    email: `${id}@example.test`,
    phone: "",
    source: "referral",
    stage,
    cvNodeId: null,
    cvFileName: null,
    cvSize: null,
    cvTrashed: false,
    retainUntil: "2027-02-01",
    retentionExpired: over,
    createdAt: "2026-08-02T09:00:00Z",
    updatedAt: "2026-08-02T09:00:00Z",
  };
}

const AMARA = person("ap-1", "Amara Diallo", "applied");
const BAS = person("ap-2", "Bas Jansen", "interview", true);

/** Deliberately NOT the seven the store has today: the board must draw what the
 *  server served, so a build that changes the vocabulary changes the columns
 *  without a web release. */
const STAGES = ["applied", "interview", "hired"];

const fakeFetch = vi.fn((url: string, init?: RequestInit) => {
  const method = init?.method ?? "GET";
  calls.push({
    url,
    method,
    body: typeof init?.body === "string" ? JSON.parse(init.body) : undefined,
  });
  const body =
    method === "DELETE"
      ? { erased: true }
      : method !== "GET"
        ? url.includes("/move")
          ? { applicant: { ...AMARA, stage: "interview" } }
          : url.includes("/applicants")
            ? { applicant: person("ap-3", "Chidi Okafor", "applied") }
            : { opening: OPENING }
        : url.includes("/hr/openings/")
          ? { applicants: [AMARA, BAS], stages: STAGES }
          : url.includes("/hr/openings")
            ? { openings: [OPENING] }
            : { applicant: AMARA, notes: [], stages: STAGES };
  return Promise.resolve(
    new Response(JSON.stringify(body), {
      status: 200,
      headers: { "content-type": "application/json" },
    }),
  );
});

// A fresh function per test, because the module caches what the approvals doors
// answered against the session's own fetch (`queues.ts`): one signed-in person,
// one answer. Rotating it here is what makes each test a different session
// rather than the same one with a different role bolted on.
let session = fakeFetch as (url: string, init?: RequestInit) => Promise<Response>;

vi.mock("../auth", () => ({
  // `session` itself, never a wrapper: a new function per render would re-create
  // every module client and loop the effects keyed on them.
  useAuth: () => ({ authorizedFetch: session, identity: { sub: "u-1", email: "", name: "" } }),
}));

vi.mock("../jmap", () => ({
  useJmapClient: () => ({
    canWorkHr: () => Promise.resolve(isHr),
    // The module resolves the approvals doors too (B6.07). This user works
    // neither the books nor the tenant, so the only queue they could have is
    // leave, and the hiring screens below are unaffected either way.
    canWorkTheBooks: () => Promise.resolve(false),
    isAdmin: () => Promise.resolve(false),
    driveDownload: () => Promise.resolve(new Blob(["cv"])),
  }),
}));

vi.mock("../drive", () => ({ saveBlob: vi.fn() }));

/** The module as it is really mounted: at `/hr/*`, routing itself. */
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

function writes(): Call[] {
  return calls.filter((c) => c.method !== "GET");
}

/** The cards of one column, by the column's own accessible name. */
function column(stage: string): HTMLElement {
  return screen.getByRole("list", { name: stage });
}

beforeEach(() => {
  calls.length = 0;
  isHr = true;
  session = (url: string, init?: RequestInit) => fakeFetch(url, init);
  fakeFetch.mockClear();
});

afterEach(cleanup);

describe("the hiring board", () => {
  test("draws the stages the server served, with each person in their column", async () => {
    ui();
    await screen.findByText(AMARA.name);
    // Three columns, because the API said three — not the seven this build's
    // store happens to have.
    //
    // `findAllByRole`: the first card arriving does not mean every column has
    // rendered, so counting synchronously straight after the await is a race an
    // idle machine wins and a loaded runner loses. The count is unchanged — a
    // fourth column still fails this.
    expect(await screen.findAllByRole("list")).toHaveLength(STAGES.length);
    expect(within(column(strings.hrStageApplied)).getByText(AMARA.name)).toBeTruthy();
    expect(within(column(strings.hrStageInterview)).getByText(BAS.name)).toBeTruthy();
    // The retention flag is the server's, and it is the only tone a card wears.
    expect(
      within(column(strings.hrStageInterview)).getByText(strings.hrRetentionExpired),
    ).toBeTruthy();
  });

  test("a drag is exactly one move request, in the server's own words", async () => {
    ui();
    const card = await screen.findByText(AMARA.name);
    const target = column(strings.hrStageHired);
    fireEvent.dragStart(card);
    fireEvent.dragOver(target);
    fireEvent.drop(target);

    await waitFor(() => expect(writes()).toHaveLength(1));
    const move = writes()[0] as Call;
    expect(move.method).toBe("POST");
    expect(move.url).toContain(`/hr/applicants/${AMARA.id}/move`);
    expect(move.body).toEqual({ stage: "hired" });
  });

  test("a drop on the column somebody is already in asks nothing", async () => {
    ui();
    const card = await screen.findByText(AMARA.name);
    fireEvent.dragStart(card);
    fireEvent.drop(column(strings.hrStageApplied));
    await waitFor(() => expect(screen.getByText(BAS.name)).toBeTruthy());
    expect(writes()).toHaveLength(0);
  });

  test("recording somebody posts to the round on screen and opens their record", async () => {
    ui();
    await screen.findByText(AMARA.name);
    fireEvent.click(screen.getByText(strings.hrAddCandidate));

    const form = screen.getByRole("dialog", { name: strings.hrAddCandidate });
    fireEvent.change(within(form).getByLabelText(strings.hrFieldName), {
      target: { value: "Chidi Okafor" },
    });
    fireEvent.click(within(form).getByText(strings.hrCreate));

    await waitFor(() => expect(writes()).toHaveLength(1));
    const recorded = writes()[0] as Call;
    expect(recorded.method).toBe("POST");
    expect(recorded.url).toContain(`/hr/openings/${OPENING.id}/applicants`);
    expect(recorded.body).toEqual({ name: "Chidi Okafor" });
  });

  test("the candidate's agent panel is in the drawer, citing where they applied from", async () => {
    ui();
    fireEvent.click(await screen.findByText(AMARA.name));

    expect(await screen.findByText(strings.recordAgentTitle)).toBeTruthy();
    expect(
      screen.getByText(strings.recordAgentOriginFrom(AMARA.source)),
    ).toBeTruthy();
  });

  test("erasing a record asks first, and then really deletes it", async () => {
    ui();
    fireEvent.click(await screen.findByText(AMARA.name));
    // The drawer reads the record, the notes and the stage vocabulary in one go.
    await screen.findByText(strings.hrNotesEmpty);

    fireEvent.click(screen.getByText(strings.hrErase));
    const asked = await screen.findByRole("dialog", { name: strings.hrErase });
    expect(asked.textContent).toContain(AMARA.name);
    // Nothing has been sent while the question is on screen.
    expect(writes()).toHaveLength(0);

    fireEvent.click(within(asked).getByRole("button", { name: strings.hrErase }));
    await waitFor(() => expect(writes()).toHaveLength(1));
    const erased = writes()[0] as Call;
    expect(erased.method).toBe("DELETE");
    expect(erased.url).toContain(`/hr/applicants/${AMARA.id}`);
  });

  test("a member who is not HR is shown no board and reads no hiring data", async () => {
    isHr = false;
    ui();
    // They land on their own leave, which is every member's and the reason the
    // module is not behind a role at all (B6.08b).
    await screen.findByText(strings.hrTabLeave);
    expect(screen.getByText(strings.hrTabDirectory)).toBeTruthy();
    expect(screen.queryByText(strings.hrTabHiring)).toBeNull();
    expect(calls.some((c) => c.url.includes("/hr/openings"))).toBe(false);
  });
});
