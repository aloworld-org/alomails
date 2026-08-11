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

/** A policy leave comes off, as `/hr/leave-policies` and the balances read
 *  serve it.
 *
 *  The screens read four of these fields — the name, whether it needs deciding,
 *  whether it is paid, and whether the tenant still runs it. The rest are the
 *  server's working and are named here so the shape is the server's shape and
 *  not a subset a later screen would have to widen. */
export interface HrLeavePolicy {
  id: string;
  name: string;
  /** `annual`, `sick`, `unpaid`, … the store's vocabulary, shown verbatim when
   *  this build does not know the word. */
  kind: string;
  entitlementMinutes: number;
  accrual: string;
  leaveYearStartMonth: number;
  leaveYearStartDay: number;
  carryoverCapMinutes: number;
  carryoverExpiresAfterMonths: number | null;
  allowNegative: boolean;
  /** False for a policy the tenant records rather than decides — sick leave,
   *  usually. A request on one lands `approved` on the spot. */
  requiresApproval: boolean;
  paid: boolean;
  archived: boolean;
  archivedAt: string | null;
  createdAt: string;
  updatedAt: string;
}

/** One policy's balance for one person, **with its working**.
 *
 *  Everything here is minutes, plus the same figures in **tenths of a day** as
 *  integers — `125` is 12.5 days. Both come from the server, and neither is
 *  divided again here: the divisor is this person's own average working day
 *  (`averageDayMinutes`), which a browser has no way to know and no business
 *  guessing. Money learned this in B1 and a holiday balance has the same
 *  reason — a person told two different numbers about their own leave stops
 *  believing both. */
export interface HrPolicyBalance {
  policy: HrLeavePolicy;
  entitlementMinutes: number;
  carriedInMinutes: number;
  accruedMinutes: number;
  takenMinutes: number;
  bookedMinutes: number;
  pendingMinutes: number;
  remainingMinutes: number;
  averageDayMinutes: number;
  entitlementDaysTenths: number;
  takenDaysTenths: number;
  bookedDaysTenths: number;
  pendingDaysTenths: number;
  remainingDaysTenths: number;
}

/** What somebody has left, on a stated day.
 *
 *  `on` is **the server's day** — the answer echoes the day it folded to, and
 *  when the caller named none that day is the server's own today. The leave
 *  screen uses it as its clock, so whether a booked absence has already begun
 *  is decided by the same calendar the server would refuse the cancellation
 *  with, rather than by the reader's device. */
export interface HrLeaveBalances {
  employeeId: string;
  /** `YYYY-MM-DD`. */
  on: string;
  balances: HrPolicyBalance[];
}

/** Somebody who is not here on a given day. A name and an id — the whole of
 *  what the absence layer says about a person, by construction: the store's
 *  query does not select the policy, the kind or the note. */
export interface HrAbsentPerson {
  employeeId: string;
  name: string;
}

/** One day of the absence layer. Days with nobody away are not served at
 *  all. */
export interface HrAbsenceDay {
  /** `YYYY-MM-DD`. */
  day: string;
  people: HrAbsentPerson[];
}

/** A day the tenant does not work, as `/hr/holidays` serves it. */
export interface HrHoliday {
  /** `YYYY-MM-DD`. */
  date: string;
  /** A stable key for the day — `christmas`, `easter_monday`. */
  key: string;
  /** The day's name in the calendar's own language. */
  name: string;
}

/** What somebody is asking for. `employeeId` is HR's alone: filing leave for a
 *  person with no login is the one reason it exists, and the server refuses it
 *  from anybody else. */
export interface LeaveDraft {
  policyId: string;
  fromDay: string;
  toDay: string;
  note?: string;
  employeeId?: string;
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

/** A CV on its way to the record: an already-uploaded blob, and the name it
 *  will read under in the tenant's HR area.
 *
 *  The upload is `POST /jmap/upload/{accountId}` like every other file in the
 *  product; the record route is what turns the blob into a Drive node **in the
 *  HR area**, which is the only place a client can put one. Nothing reads it —
 *  see `docs/design/hr.md` § The EU AI Act posture. */
export interface CvUpload {
  blobId: string;
  name: string;
  size: number;
  contentType: string | null;
}

/** The writable fields of an application. `email: null` clears the address;
 *  `cv: null` takes the one on file off (and trashes it); `stage` is
 *  deliberately absent — moving somebody is its own act. */
export interface ApplicantDraft {
  name?: string;
  email?: string | null;
  phone?: string;
  source?: string;
  cv?: CvUpload | null;
  retainUntil?: string;
}

/** The terms somebody starts on, as `POST /hr/employees` takes them beside the
 *  person.
 *
 *  `startedOn` is required by the server and never defaulted to today here: a
 *  start date is a fact about a contract, and every leave balance is folded
 *  from it. `contractKind` is the same closed vocabulary an opening's
 *  `employmentKind` is drawn from, which is why a hire can carry the round's
 *  word across unchanged. */
export interface EmploymentDraft {
  /** `YYYY-MM-DD`. */
  startedOn: string;
  jobTitle?: string;
  team?: string;
  contractKind?: string;
}

/** A new employee record, as this module writes one.
 *
 *  Deliberately a **narrow** view of a wide route: the create route accepts a
 *  home address, a date of birth, a national id and a bank account, and the
 *  hire bridge sends none of them. What a hiring board legitimately knows about
 *  somebody is their name and how to write to them; the rest is HR's to enter
 *  on the record screen, with the person in front of them. */
export interface EmployeeDraft {
  givenName: string;
  familyName: string;
  workEmail?: string;
  employment: EmploymentDraft;
}

/** The employee a create answered with — the two fields this module needs of a
 *  record whose other thirty are HR's own screen's. */
export interface HrCreatedEmployee {
  id: string;
  /** How the tenant writes their name, preferred name applied — the server's
   *  own projection, never joined together here. */
  name: string;
}
