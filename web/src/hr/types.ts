// What `/hr` answers about hiring, as the screens read it (alo HR, ADR 0035,
// wave B6.06b).
//
// These are the server's own JSON shapes, named — not a model of our own. Two
// vocabularies show why that matters:
//
//   - **stages are served, never assumed.** `GET …/applicants` answers a
//     `stages` list in board order, and the board draws exactly those columns.
//     A build that gains an eighth stage grows a column here without a web
//     release (`docs/design/hr.md` § Recruitment-lite).
//   - **employment kinds are a fixed list**, because the API does not serve
//     one. The picker offers the six the store accepts and the server refuses
//     anything else with a `422` naming the set — so a stale client shows a
//     sentence rather than writing a word nobody knows.

/** Where an opening stands. `closed` is terminal and freezes the record. */
export type OpeningStatus = "draft" | "open" | "closed";

/** The contract an opening is for. The six words the store accepts, in the
 *  order a picker reads best; unknown words that arrive from a newer server are
 *  shown verbatim rather than hidden. */
export const EMPLOYMENT_KINDS = [
  "permanent",
  "fixed_term",
  "part_time",
  "apprentice",
  "contractor",
  "intern",
] as const;

/** A role a tenant is hiring for. */
export interface HrOpening {
  id: string;
  title: string;
  team: string;
  location: string;
  employmentKind: string;
  status: OpeningStatus;
  /** `YYYY-MM-DD`, set when the round was published. */
  openedOn: string | null;
  /** `YYYY-MM-DD`, set when the round ended. */
  closedOn: string | null;
  /** How many people applied — the server's count, never a length of a list
   *  this screen happens to hold. */
  applicants: number;
  createdBy: string;
  createdAt: string;
  updatedAt: string;
}

/** Somebody who applied. `stage` is the served vocabulary's word, and it moves
 *  only through the move route. */
export interface HrApplicant {
  id: string;
  openingId: string;
  name: string;
  email: string | null;
  phone: string;
  source: string;
  stage: string;
  /** The Drive node their CV became, in the tenant's HR-only area. Never read
   *  by anything — see `docs/design/hr.md` § The EU AI Act posture. */
  cvNodeId: string | null;
  cvFileName: string | null;
  cvSize: number | null;
  /** True when the file has since been trashed in Drive; the record stays as
   *  the honest statement that there was one. */
  cvTrashed: boolean;
  /** `YYYY-MM-DD`: the day after which this record may be erased. */
  retainUntil: string;
  /** The server's own reading of `retainUntil` against today — never recomputed
   *  here, because a browser's clock is not the record's. */
  retentionExpired: boolean;
  createdAt: string;
  updatedAt: string;
}

/** What somebody who met the candidate wrote, with their id on it. */
export interface HrApplicantNote {
  id: string;
  author: string;
  body: string;
  createdAt: string;
}

/** One opening's pipeline, with the vocabulary its columns are drawn from. */
export interface HrPipeline {
  applicants: HrApplicant[];
  stages: string[];
}

/** One candidate, everything written about them, and where they can be moved
 *  to. */
export interface HrApplicantDetail {
  applicant: HrApplicant;
  notes: HrApplicantNote[];
  stages: string[];
}

/** Where a leave request has got to. `requested` is the only state an approver
 *  can act on; the other four are the record of what happened. */
export type LeaveStatus = "requested" | "approved" | "rejected" | "withdrawn" | "cancelled";

/** One person's ask for time off, as `/hr/leave-requests` serves it.
 *
 *  `costMinutes`, `workingDays` and `holidayMinutes` are **the server's fold**
 *  over the person's working pattern and the tenant's public holidays. Nothing
 *  in the browser recomputes them: a week that costs four days rather than five
 *  has a reason, and the reason lives where the calendar does. */
export interface HrLeaveRequest {
  id: string;
  employeeId: string;
  /** Who is asking, as the directory names them. */
  employeeName: string;
  policyId: string;
  /** Which policy the days come off — "Annual leave", "Sick leave". */
  policyName: string;
  /** `YYYY-MM-DD`, both ends inclusive. */
  fromDay: string;
  toDay: string;
  status: LeaveStatus;
  /** What the person wrote for whoever decides. Personal, and never logged. */
  note: string;
  costMinutes: number;
  workingDays: number;
  /** What the tenant's public holidays saved inside the dates. */
  holidayMinutes: number;
  decidedBy: string | null;
  decidedAt: string | null;
  decisionNote: string;
  closedAt: string | null;
  createdAt: string;
  updatedAt: string;
}

/** One person as the directory shows them: **the public fields only**, and
 *  structurally so — the server folds this from a type that has no home address
 *  on it to leak (`hr_employees::directory_json`). What is missing here is the
 *  point of the type.
 *
 *  The inbox reads exactly one of them — `managerId`, to know whose leave is its
 *  caller's to decide; the directory screen reads the rest. */
export interface HrDirectoryEntry {
  id: string;
  /** How the tenant writes their name, preferred name applied. */
  name: string;
  givenName: string;
  familyName: string;
  preferredName: string;
  workEmail: string;
  workPhone: string;
  managerId: string | null;
  /** The Drive node their photo is, when the tenant filed one. Nothing renders
   *  it yet — the directory draws initials (`ds/Avatar`). */
  photoNodeId: string | null;
  jobTitle: string;
  team: string;
  /** `YYYY-MM-DD`, the day their current employment began, or `null` for
   *  somebody with no terms written down. */
  startedOn: string | null;
  archived: boolean;
}

/** The people list as `/hr/employees` answers it: the rows, and whether the
 *  caller asked through the HR door.
 *
 *  `hr` is what draws the "include people who have left" control and nothing
 *  else. It is **not** the access decision: the route ignores `includeArchived`
 *  for anybody else rather than refusing it, so a stale `true` here shows a
 *  checkbox that quietly changes nothing instead of opening a record. */
export interface HrDirectory {
  employees: HrDirectoryEntry[];
  hr: boolean;
}

/** One person in the reporting tree, with the people beneath them.
 *
 *  The tree is **the server's**, not a fold of `managerId` done here: somebody
 *  whose manager has left is served as a root rather than dropped, and the store
 *  refuses a reporting line that would close a cycle — so this structure is
 *  finite by construction and a browser never has to defend against one that is
 *  not. Active people only; archived colleagues are in the list, never in the
 *  chart. */
export interface HrOrgNode {
  id: string;
  name: string;
  jobTitle: string;
  team: string;
  managerId: string | null;
  reports: HrOrgNode[];
}

/** The caller's own HR standing: their employee record when the tenant has one
 *  for them, and whether they hold the HR door. */
export interface HrMe {
  /** `null` for a login with no employee record — a contractor with a mailbox,
   *  an admin who is not on the payroll. An ordinary answer, not an error. */
  employee: { id: string; name: string } | null;
  isHr: boolean;
}

/** The writable fields of an opening. An absent field keeps what is stored. */
export interface OpeningDraft {
  title?: string;
  team?: string;
  location?: string;
  employmentKind?: string;
}

/** The writable fields of an application. `email: null` clears the address;
 *  `stage` is deliberately absent — moving somebody is its own act. */
export interface ApplicantDraft {
  name?: string;
  email?: string | null;
  phone?: string;
  source?: string;
  retainUntil?: string;
}
