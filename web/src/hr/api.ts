// The client for the `/hr` HTTP surface (alo HR, ADR 0035, wave B6).
//
// Its own small client, for the reason Billing, CRM, Projects, Finance and
// Inventory each have one: `/hr` is a plain REST surface with none of JMAP's
// session, capabilities or method-call envelope. It uses the same authenticated
// fetch, so there is one session and not two, and it fails through the shared
// `platform/rest` shape so a server sentence reaches a user the same way in
// every module.
//
// It holds NO rules and NO derived facts. Whether a candidate's record is past
// its retention date, which stages exist, whether an opening may still take
// applications — all of it is the server's, asked for and shown. And nothing
// here reads a CV: the only thing this file does with one is upload the blob
// through the record's own route and hand back the name it was given.
import { useMemo } from "react";

import { useAuth } from "../auth";
import { API_BASE } from "../platform/runtime";
import { RestError, problemDetail, restMessage } from "../platform/rest";
import type {
  ApplicantDraft,
  EmployeeDraft,
  HrAbsenceDay,
  HrApplicant,
  HrApplicantDetail,
  HrApplicantNote,
  HrCreatedEmployee,
  HrDirectory,
  HrDirectoryEntry,
  HrHoliday,
  HrLeaveBalances,
  HrLeaveRequest,
  HrLetterTemplate,
  HrLetterTemplateCatalog,
  HrLetterTemplateDraft,
  HrMe,
  HrOpening,
  HrOrgNode,
  HrPipeline,
  LeaveDraft,
  LeaveStatus,
  OpeningDraft,
} from "./types";

type AuthorizedFetch = (input: string, init?: RequestInit) => Promise<Response>;

/** A failed HR request, carrying the server's own `Problem` detail. */
export class HrError extends RestError {
  constructor(status: number, detail: string | null) {
    super(status, detail, "HrError");
  }
}

/** What to show a user about a failed request: the server's sentence when it
 *  sent one, `fallback` otherwise. */
export function hrMessage(error: unknown, fallback: string): string {
  return restMessage(error, fallback);
}

/** The tenant's openings, the people who applied for them, the notes on those
 *  people — and the leave somebody has asked for, with the two decisions that
 *  answer it. One instance per auth context. */
export class HrApi {
  readonly #fetch: AuthorizedFetch;

  constructor(authorizedFetch: AuthorizedFetch) {
    this.#fetch = authorizedFetch;
  }

  /** What this tenant is hiring for. Rounds that are over are left out unless
   *  they are asked for: a board of last year's roles is not a hiring board. */
  openings(includeClosed = false): Promise<HrOpening[]> {
    const suffix = includeClosed ? "?includeClosed=1" : "";
    return this.#read<{ openings?: HrOpening[] }>(`/hr/openings${suffix}`).then(
      (r) => r.openings ?? [],
    );
  }

  /** Writes down a role. It starts as a draft — a company writes the role
   *  before it decides to run it. */
  createOpening(draft: OpeningDraft): Promise<HrOpening> {
    return this.#write<{ opening: HrOpening }>(
      "POST",
      "/hr/openings",
      draft,
    ).then((r) => r.opening);
  }

  /** Corrects a role. A closed round refuses: rewriting the title of a role
   *  thirty people applied for would rewrite what they applied for. */
  updateOpening(id: string, draft: OpeningDraft): Promise<HrOpening> {
    return this.#write<{ opening: HrOpening }>(
      "PATCH",
      `/hr/openings/${encodeURIComponent(id)}`,
      draft,
    ).then((r) => r.opening);
  }

  /** The round is running from today. */
  publishOpening(id: string): Promise<HrOpening> {
    return this.#write<{ opening: HrOpening }>(
      "POST",
      `/hr/openings/${encodeURIComponent(id)}/publish`,
      {},
    ).then((r) => r.opening);
  }

  /** The round is over, however it ended. The applicants stay: they are the
   *  record of what happened. */
  closeOpening(id: string): Promise<HrOpening> {
    return this.#write<{ opening: HrOpening }>(
      "POST",
      `/hr/openings/${encodeURIComponent(id)}/close`,
      {},
    ).then((r) => r.opening);
  }

  /** One opening's pipeline **and** the stage vocabulary its columns are drawn
   *  from, in one read — so a board never hard-codes the columns. */
  pipeline(openingId: string): Promise<HrPipeline> {
    return this.#read<{ applicants?: HrApplicant[]; stages?: string[] }>(
      `/hr/openings/${encodeURIComponent(openingId)}/applicants`,
    ).then((r) => ({ applicants: r.applicants ?? [], stages: r.stages ?? [] }));
  }

  /** Somebody applied. They land at the first stage whatever this form says —
   *  every move after that is a person's act. */
  recordApplicant(
    openingId: string,
    draft: ApplicantDraft,
  ): Promise<HrApplicant> {
    return this.#write<{ applicant: HrApplicant }>(
      "POST",
      `/hr/openings/${encodeURIComponent(openingId)}/applicants`,
      draft,
    ).then((r) => r.applicant);
  }

  /** One candidate, what was written about them, and where they can be moved
   *  to. A candidate of another tenant is the same `404` an id that never
   *  existed gets. */
  applicant(id: string): Promise<HrApplicantDetail> {
    return this.#read<HrApplicantDetail>(
      `/hr/applicants/${encodeURIComponent(id)}`,
    );
  }

  /** Corrects what was recorded. It cannot move anybody: a fixed telephone
   *  number must never be able to reorder somebody's candidacy. */
  updateApplicant(id: string, draft: ApplicantDraft): Promise<HrApplicant> {
    return this.#write<{ applicant: HrApplicant }>(
      "PATCH",
      `/hr/applicants/${encodeURIComponent(id)}`,
      draft,
    ).then((r) => r.applicant);
  }

  /** Where a person decided this candidate now stands. Audited with the
   *  deciding person's id on it — which is the point. */
  moveApplicant(id: string, stage: string): Promise<HrApplicant> {
    return this.#write<{ applicant: HrApplicant }>(
      "POST",
      `/hr/applicants/${encodeURIComponent(id)}/move`,
      { stage },
    ).then((r) => r.applicant);
  }

  /** An interview note, with its author on it. Notes are never edited or
   *  deleted on their own — they go when the record goes. */
  addNote(id: string, body: string): Promise<HrApplicantNote> {
    return this.#write<{ note: HrApplicantNote }>(
      "POST",
      `/hr/applicants/${encodeURIComponent(id)}/notes`,
      { body },
    ).then((r) => r.note);
  }

  /** The retention deadline, acted on: the record, its notes and its CV go.
   *  The one HR record that is deleted rather than archived, and nothing calls
   *  it on a timer — a person presses the button. */
  async eraseApplicant(id: string): Promise<void> {
    await this.#json<unknown>(
      await this.#send(`/hr/applicants/${encodeURIComponent(id)}`, {
        method: "DELETE",
      }),
    );
  }

  /** Somebody who took the job, written into the directory as a colleague.
   *
   *  The person and the terms they start on travel in **one act**, because the
   *  create route reads `employment` on create only: a second call would be a
   *  second failure point between a colleague existing and their start date
   *  existing, and a record with no start date has no leave balance to read.
   *
   *  It creates no login and no mailbox, deliberately — a write path from HR
   *  into identity would make a badly-scoped HR permission into an
   *  account-creation capability (`docs/design/hr.md` § Out of scope). */
  createEmployee(draft: EmployeeDraft): Promise<HrCreatedEmployee> {
    return this.#write<{ employee: HrCreatedEmployee }>(
      "POST",
      "/hr/employees",
      draft,
    ).then((r) => r.employee);
  }

  /** The caller's own HR standing. Two facts, and there is no argument by which
   *  this route could ask about a colleague. */
  me(): Promise<HrMe> {
    return this.#read<HrMe>("/hr/me");
  }

  /** The people list, public fields only — every member's read, and the one the
   *  directory screen is drawn from. The approvals inbox uses it for exactly one
   *  thing: whether anybody's `managerId` is the caller, which is what makes
   *  them somebody's approver.
   *
   *  `includeArchived` asks for the people who have left as well. It is **HR's
   *  read**: the server ignores the flag for anybody else rather than refusing
   *  it, and answers `hr` so the control that sets it is drawn only for the
   *  people it does something for. */
  directory(includeArchived = false): Promise<HrDirectory> {
    const suffix = includeArchived ? "?includeArchived=1" : "";
    return this.#read<{ employees?: HrDirectoryEntry[]; hr?: boolean }>(
      `/hr/employees${suffix}`,
    ).then((r) => ({ employees: r.employees ?? [], hr: r.hr === true }));
  }

  /** The reporting tree of this tenant's active people, roots first — every
   *  member's read, and the whole of the org chart. The shape is the server's
   *  fold and is never rebuilt here from `managerId`: a person whose manager has
   *  left is a root in that answer, and re-deriving it in a browser is how a
   *  branch goes missing. */
  orgChart(): Promise<HrOrgNode[]> {
    return this.#read<{ chart?: HrOrgNode[] }>("/hr/org").then(
      (r) => r.chart ?? [],
    );
  }

  /** Leave the caller may see, in the scope they are asking as: `mine` is their
   *  own, `team` is the people who report to them, `all` is HR's.
   *
   *  **The scope is the server's rule, not a filter applied here**: asking for
   *  `all` without the HR role is a `403`, and asking for `team` names the
   *  caller's own reports server-side. `statuses` narrows what comes back —
   *  `requested` is the approvals inbox's, because it is the only state
   *  anybody can act on. */
  leaveRequests(
    scope: "mine" | "team" | "all",
    statuses: LeaveStatus[] = [],
  ): Promise<HrLeaveRequest[]> {
    const query = new URLSearchParams({ scope });
    if (statuses.length > 0) query.set("status", statuses.join(","));
    return this.#read<{ requests?: HrLeaveRequest[] }>(
      `/hr/leave-requests?${query.toString()}`,
    ).then((r) => r.requests ?? []);
  }

  /** Yes to somebody's time off. The balance, the overlap and who may decide
   *  are all the server's — this sends the act and shows what came back. */
  approveLeaveRequest(id: string, note?: string): Promise<HrLeaveRequest> {
    return this.#decideLeave(id, "approve", note);
  }

  /** No, with the sentence the person is going to read. */
  rejectLeaveRequest(id: string, note: string): Promise<HrLeaveRequest> {
    return this.#decideLeave(id, "reject", note);
  }

  /** Asking for time off. Every rule that could refuse it — the balance, days
   *  another request already covers, a policy that has been retired, dates
   *  outside somebody's employment — is the server's, and its sentence is what
   *  the form shows. */
  createLeaveRequest(draft: LeaveDraft): Promise<HrLeaveRequest> {
    return this.#write<{ request: HrLeaveRequest }>(
      "POST",
      "/hr/leave-requests",
      draft,
    ).then((r) => r.request);
  }

  /** Taking back a request nobody has decided. The person who asked, and
   *  nobody else — a manager who wants different dates rejects with a note. */
  withdrawLeaveRequest(id: string): Promise<HrLeaveRequest> {
    return this.#decideLeave(id, "withdraw");
  }

  /** Approved leave, given back. It returns the balance, which is what makes
   *  approving undoable and therefore unconfirmed; leave that has already begun
   *  is the server's `409`. */
  cancelLeaveRequest(id: string): Promise<HrLeaveRequest> {
    return this.#decideLeave(id, "cancel");
  }

  /** What somebody has left, on a day — **and the day the server folded to**,
   *  which is this module's clock (`HrLeaveBalances.on`).
   *
   *  One entry per live policy, each with its whole working. Absent
   *  `employeeId` means the caller's own; naming somebody else is the manager's
   *  or HR's read and the server decides which. */
  leaveBalances(employeeId?: string, on?: string): Promise<HrLeaveBalances> {
    const query = new URLSearchParams();
    if (employeeId !== undefined && employeeId !== "")
      query.set("employeeId", employeeId);
    if (on !== undefined && on !== "") query.set("on", on);
    const suffix = query.toString() === "" ? "" : `?${query.toString()}`;
    return this.#read<Partial<HrLeaveBalances>>(
      `/hr/leave-balances${suffix}`,
    ).then((r) => ({
      employeeId: r.employeeId ?? "",
      on: r.on ?? "",
      balances: r.balances ?? [],
    }));
  }

  /** Who is away between two days, both ends inclusive — every member's read,
   *  and the module's one read about other people. Days with nobody away are
   *  not served, so the answer is short even for a month. */
  absences(from: string, to: string): Promise<HrAbsenceDay[]> {
    const query = new URLSearchParams({ from, to });
    return this.#read<{ days?: HrAbsenceDay[] }>(
      `/hr/absences?${query.toString()}`,
    ).then((r) => r.days ?? []);
  }

  /** The days the tenant does not work in one year, on its own calendar. A
   *  company that observes nothing answers an empty list rather than an error,
   *  and the calendar simply marks nothing. */
  holidays(year: number): Promise<HrHoliday[]> {
    return this.#read<{ holidays?: HrHoliday[] }>(
      `/hr/holidays?year=${year}`,
    ).then((r) => r.holidays ?? []);
  }

  /** The tenant's approved letter patterns and the merge vocabulary. */
  letterTemplates(): Promise<HrLetterTemplateCatalog> {
    return this.#read<Partial<HrLetterTemplateCatalog>>(
      "/hr/letter-templates",
    ).then((r) => ({
      templates: r.templates ?? [],
      fields: r.fields ?? [],
    }));
  }

  createLetterTemplate(
    draft: HrLetterTemplateDraft,
  ): Promise<HrLetterTemplate> {
    return this.#write<{ template: HrLetterTemplate }>(
      "POST",
      "/hr/letter-templates",
      draft,
    ).then((r) => r.template);
  }

  updateLetterTemplate(
    id: string,
    draft: HrLetterTemplateDraft,
  ): Promise<HrLetterTemplate> {
    return this.#write<{ template: HrLetterTemplate }>(
      "PATCH",
      `/hr/letter-templates/${encodeURIComponent(id)}`,
      draft,
    ).then((r) => r.template);
  }

  async deleteLetterTemplate(id: string): Promise<void> {
    await this.#write<{ deleted: boolean }>(
      "DELETE",
      `/hr/letter-templates/${encodeURIComponent(id)}`,
      {},
    );
  }

  #decideLeave(
    id: string,
    verb: string,
    note?: string,
  ): Promise<HrLeaveRequest> {
    return this.#write<{ request: HrLeaveRequest }>(
      "POST",
      `/hr/leave-requests/${encodeURIComponent(id)}/${verb}`,
      { note: note ?? "" },
    ).then((r) => r.request);
  }

  async #read<T>(path: string): Promise<T> {
    return this.#json<T>(await this.#send(path, { method: "GET" }));
  }

  async #write<T>(method: string, path: string, body: unknown): Promise<T> {
    return this.#json<T>(
      await this.#send(path, {
        method,
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      }),
    );
  }

  async #send(path: string, init: RequestInit): Promise<Response> {
    try {
      return await this.#fetch(`${API_BASE}${path}`, init);
    } catch {
      // A dropped connection is not a status code; give it one the UI can treat
      // like any other failure rather than an unhandled rejection.
      throw new HrError(0, null);
    }
  }

  async #json<T>(res: Response): Promise<T> {
    if (!res.ok) throw new HrError(res.status, await problemDetail(res));
    return (await res.json()) as T;
  }
}

/** The HR client bound to the current session. Memoized per auth context, so a
 *  re-render never re-creates it and effects keyed on it do not loop. */
export function useHrApi(): HrApi {
  const { authorizedFetch } = useAuth();
  return useMemo(() => new HrApi(authorizedFetch), [authorizedFetch]);
}
