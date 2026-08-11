// What the two acts B6.08c adds actually do on the wire: attaching a CV to a
// candidate's record, and writing somebody who took the job into the directory.
//
// Only the network, the session's door answers and the blob upload are faked.
// The real router, the real module routes, the real client, the real board, the
// real drawer and the real forms all run — the point of the item is that these
// screens agree with the API, and a test against stubs could not tell.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

import { DialogProvider } from "../ds";
import { strings } from "../i18n";
import { HrModule } from "./HrModule";
import type { HrApplicant, HrDirectoryEntry, HrOpening } from "./types";

interface Call {
  url: string;
  method: string;
  body: unknown;
}

const calls: Call[] = [];

const OPENING: HrOpening = {
  id: "op-1",
  title: "Backend engineer",
  team: "Platform",
  location: "Rotterdam",
  employmentKind: "fixed_term",
  status: "open",
  openedOn: "2026-08-01",
  closedOn: null,
  applicants: 2,
  createdBy: "u-1",
  createdAt: "2026-08-01T09:00:00Z",
  updatedAt: "2026-08-01T09:00:00Z",
};

function person(id: string, name: string, stage: string, cv = false): HrApplicant {
  return {
    id,
    openingId: OPENING.id,
    name,
    email: `${id}@example.test`,
    phone: "",
    source: "referral",
    stage,
    cvNodeId: cv ? "dn-1" : null,
    cvFileName: cv ? "amara-cv.pdf" : null,
    cvSize: cv ? 4096 : null,
    cvTrashed: false,
    retainUntil: "2027-02-01",
    retentionExpired: false,
    createdAt: "2026-08-02T09:00:00Z",
    updatedAt: "2026-08-02T09:00:00Z",
  };
}

/** Hired, and therefore the one the bridge is offered about. */
const AMARA = person("ap-1", "Amara van den Berg", "hired");
/** Still being met — no bridge, whatever else is true of them. */
const BAS = person("ap-2", "Bas Jansen", "interview");

const STAGES = ["applied", "interview", "hired"];

/** Who the directory answers with. The address is deliberately the one the
 *  hired candidate applied with, so the duplicate warning has something to
 *  find when a test asks for it. */
let directory: HrDirectoryEntry[] = [];

function colleague(over: Partial<HrDirectoryEntry> = {}): HrDirectoryEntry {
  return {
    id: "em-9",
    name: "Amara van den Berg",
    givenName: "Amara",
    familyName: "van den Berg",
    preferredName: "",
    workEmail: "ap-1@example.test",
    workPhone: "",
    managerId: null,
    photoNodeId: null,
    jobTitle: "Backend engineer",
    team: "Platform",
    startedOn: "2024-01-08",
    archived: false,
    ...over,
  };
}

/** The record the drawer is opened on. A test that needs the other candidate
 *  points this at them before pressing their card. */
let opened: HrApplicant = AMARA;

const fakeFetch = vi.fn((url: string, init?: RequestInit) => {
  const method = init?.method ?? "GET";
  calls.push({
    url,
    method,
    body: typeof init?.body === "string" ? JSON.parse(init.body) : undefined,
  });
  const body = ((): unknown => {
    if (method === "POST" && url.includes("/hr/employees")) {
      return { employee: { id: "em-1", name: "Amara van den Berg" }, employments: [] };
    }
    if (method !== "GET") {
      if (url.includes("/move")) return { applicant: { ...opened, stage: "hired" } };
      if (url.includes("/applicants")) return { applicant: opened };
      return { opening: OPENING };
    }
    if (url.includes("/hr/org")) return { chart: [] };
    if (url.includes("/hr/employees")) return { employees: directory, hr: true };
    if (url.includes("/hr/openings/")) return { applicants: [AMARA, BAS], stages: STAGES };
    if (url.includes("/hr/openings")) return { openings: [OPENING] };
    return { applicant: opened, notes: [], stages: STAGES };
  })();
  return Promise.resolve(
    new Response(JSON.stringify(body), {
      status: 200,
      headers: { "content-type": "application/json" },
    }),
  );
});

let session = fakeFetch as (url: string, init?: RequestInit) => Promise<Response>;

vi.mock("../auth", () => ({
  useAuth: () => ({ authorizedFetch: session, identity: { sub: "u-1", email: "", name: "" } }),
}));

/** What the upload answered, or the failure it threw. */
const uploadFile = vi.fn();

vi.mock("../jmap", () => ({
  useJmapClient: () => ({
    canWorkHr: () => Promise.resolve(true),
    canWorkTheBooks: () => Promise.resolve(false),
    isAdmin: () => Promise.resolve(false),
    driveDownload: () => Promise.resolve(new Blob(["cv"])),
    uploadFile,
  }),
}));

vi.mock("../drive", () => ({ saveBlob: vi.fn() }));

/** Where the app actually is — the confirmation a hire lands on. */
function Where() {
  const location = useLocation();
  return <span data-testid="where">{`${location.pathname}${location.search}`}</span>;
}

function ui(path = "/hr/hiring") {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <DialogProvider>
        <Where />
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

/** The control inside a labelled field.
 *
 *  `getByLabelText` matches a wrapping label's whole text, and every field here
 *  carries a hint under the control — so the label is found by its own words and
 *  the control taken from inside it. */
function field(scope: HTMLElement, label: string): HTMLElement {
  const wrapper = within(scope).getByText(label).closest("label");
  const control = wrapper?.querySelector("input, select, textarea");
  if (!(control instanceof HTMLElement)) throw new Error(`no control labelled ${label}`);
  return control;
}

/** Opens the drawer on the candidate the fake is serving. */
async function openDrawer(name: string): Promise<void> {
  fireEvent.click(await screen.findByText(name));
  await screen.findByText(strings.hrNotesEmpty);
}

beforeEach(() => {
  calls.length = 0;
  directory = [];
  opened = AMARA;
  session = (url: string, init?: RequestInit) => fakeFetch(url, init);
  fakeFetch.mockClear();
  uploadFile.mockReset();
  uploadFile.mockResolvedValue({ blobId: "bl-7", type: "application/pdf", size: 4096 });
});

afterEach(cleanup);

describe("the CV, from the browser", () => {
  test("a chosen file is uploaded once, and the record carries what came back", async () => {
    ui();
    await screen.findByText(AMARA.name);
    fireEvent.click(screen.getByText(strings.hrAddCandidate));
    const form = screen.getByRole("dialog", { name: strings.hrAddCandidate });
    fireEvent.change(within(form).getByLabelText(strings.hrFieldName), {
      target: { value: "Chidi Okafor" },
    });
    const file = new File(["a cv"], "chidi-cv.pdf", { type: "application/pdf" });
    fireEvent.change(field(form, strings.hrCv), { target: { files: [file] } });
    // Choosing a file uploads nothing: a form somebody closes leaves no blob.
    expect(uploadFile).not.toHaveBeenCalled();

    fireEvent.click(within(form).getByText(strings.hrCreate));
    await waitFor(() => expect(writes()).toHaveLength(1));
    expect(uploadFile).toHaveBeenCalledTimes(1);
    const recorded = writes()[0] as Call;
    expect(recorded.method).toBe("POST");
    expect(recorded.url).toContain(`/hr/openings/${OPENING.id}/applicants`);
    // The blob id and the server's own measurement of it — never the File's
    // idea of its size — with the name it will read under in the HR area.
    expect(recorded.body).toEqual({
      name: "Chidi Okafor",
      cv: {
        blobId: "bl-7",
        name: "chidi-cv.pdf",
        size: 4096,
        contentType: "application/pdf",
      },
    });
  });

  test("an upload that fails saves nothing at all", async () => {
    uploadFile.mockRejectedValue(new Error("no"));
    ui();
    await screen.findByText(AMARA.name);
    fireEvent.click(screen.getByText(strings.hrAddCandidate));
    const form = screen.getByRole("dialog", { name: strings.hrAddCandidate });
    fireEvent.change(within(form).getByLabelText(strings.hrFieldName), {
      target: { value: "Chidi Okafor" },
    });
    fireEvent.change(field(form, strings.hrCv), {
      target: { files: [new File(["a cv"], "chidi-cv.pdf", { type: "application/pdf" })] },
    });
    fireEvent.click(within(form).getByText(strings.hrCreate));

    await screen.findByText(strings.hrCvUploadFailed);
    // The candidate was not written: half a record is worse than a sentence.
    expect(writes()).toHaveLength(0);
    expect(screen.getByRole("dialog", { name: strings.hrAddCandidate })).toBeTruthy();
  });

  test("taking a CV off is an explicit null, not an absent field", async () => {
    opened = person("ap-1", "Amara van den Berg", "hired", true);
    ui();
    await openDrawer(AMARA.name);
    fireEvent.click(screen.getByLabelText(strings.hrEditCandidate));
    const form = await screen.findByRole("dialog", { name: strings.hrEditCandidate });
    fireEvent.click(within(form).getByLabelText(strings.hrCvRemove));
    fireEvent.click(within(form).getByText(strings.hrSave));

    await waitFor(() => expect(writes()).toHaveLength(1));
    const patched = writes()[0] as Call;
    expect(patched.method).toBe("PATCH");
    expect(patched.url).toContain(`/hr/applicants/${AMARA.id}`);
    expect(patched.body).toEqual({ cv: null });
    expect(uploadFile).not.toHaveBeenCalled();
  });

  test("a candidate with no CV is offered one, through the record form", async () => {
    ui();
    await openDrawer(AMARA.name);
    fireEvent.click(screen.getByText(strings.hrCvAttach));
    // One upload path: the drawer sends people to the form rather than growing
    // a control of its own that would have to agree with it.
    await screen.findByRole("dialog", { name: strings.hrEditCandidate });
    expect(writes()).toHaveLength(0);
  });
});

describe("somebody who took the job", () => {
  test("the bridge is offered on a hired candidate and on nobody else", async () => {
    opened = BAS;
    ui();
    await openDrawer(BAS.name);
    expect(screen.queryByText(strings.hrHire)).toBeNull();
    cleanup();

    opened = AMARA;
    ui();
    await openDrawer(AMARA.name);
    expect(screen.getByText(strings.hrHire)).toBeTruthy();
    // Moving the card recorded the outcome; nothing was created by it.
    expect(writes()).toHaveLength(0);
  });

  test("the form opens on the application and the round, and posts one record", async () => {
    ui();
    await openDrawer(AMARA.name);
    fireEvent.click(screen.getByText(strings.hrHire));
    const form = await screen.findByRole("dialog", { name: strings.hrHire });

    // The name split by the particle rule, and the role from the round.
    expect((field(form, strings.hrFieldGivenName) as HTMLInputElement).value).toBe(
      "Amara",
    );
    expect(
      (field(form, strings.hrFieldFamilyName) as HTMLInputElement).value,
    ).toBe("van den Berg");
    expect((field(form, strings.hrFieldJobTitle) as HTMLInputElement).value).toBe(
      "Backend engineer",
    );
    expect((field(form, strings.hrFieldEmployment) as HTMLSelectElement).value).toBe(
      "fixed_term",
    );
    // A start date is never defaulted to today: every leave balance is folded
    // from it, and a silent "now" is wrong in a way nobody notices.
    expect((field(form, strings.hrFieldStartedOn) as HTMLInputElement).value).toBe(
      "",
    );

    fireEvent.change(field(form, strings.hrFieldStartedOn), {
      target: { value: "2026-09-01" },
    });
    fireEvent.click(within(form).getByText(strings.hrHireSubmit));

    await waitFor(() => expect(writes()).toHaveLength(1));
    const created = writes()[0] as Call;
    expect(created.method).toBe("POST");
    expect(created.url).toContain("/hr/employees");
    // The person and the terms in ONE body: the create route reads `employment`
    // on create only, and a colleague with no start date has no balance to read.
    expect(created.body).toEqual({
      givenName: "Amara",
      familyName: "van den Berg",
      workEmail: "ap-1@example.test",
      employment: {
        startedOn: "2026-09-01",
        jobTitle: "Backend engineer",
        team: "Platform",
        contractKind: "fixed_term",
      },
    });

    // The confirmation is the colleague themselves, in the directory.
    await waitFor(() =>
      expect(screen.getByTestId("where").textContent).toBe(
        "/hr/directory?q=Amara%20van%20den%20Berg",
      ),
    );
  });

  test("an address that is already somebody's is named, and still allowed", async () => {
    directory = [colleague()];
    ui();
    await openDrawer(AMARA.name);
    fireEvent.click(screen.getByText(strings.hrHire));
    const form = await screen.findByRole("dialog", { name: strings.hrHire });

    // The read is HR's own, including the people who have left, because a
    // returning colleague is exactly the case worth naming.
    await waitFor(() =>
      expect(
        calls.some((c) => c.method === "GET" && c.url.includes("/hr/employees?includeArchived=1")),
      ).toBe(true),
    );
    await within(form).findByText(strings.hrHireKnown(colleague().name));

    // A warning, not a gate: the server keeps no unique index on an address,
    // and only the person here knows whether this is the same person.
    fireEvent.change(field(form, strings.hrFieldStartedOn), {
      target: { value: "2026-09-01" },
    });
    fireEvent.click(within(form).getByText(strings.hrHireSubmit));
    await waitFor(() => expect(writes()).toHaveLength(1));
  });

  test("somebody who left with this address is spoken of differently", async () => {
    directory = [colleague({ archived: true })];
    ui();
    await openDrawer(AMARA.name);
    fireEvent.click(screen.getByText(strings.hrHire));
    const form = await screen.findByRole("dialog", { name: strings.hrHire });
    await within(form).findByText(strings.hrHireKnownLeft(colleague().name));
    expect(within(form).queryByText(strings.hrHireKnown(colleague().name))).toBeNull();
  });
});
