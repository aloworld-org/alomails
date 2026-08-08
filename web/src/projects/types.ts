// The shapes the `/projects` API answers with (alo Projects, ADR 0035, wave
// B3). One interface per JSON object the server sends, named as the server
// names it — no client-side derived fields, because a field this file invented
// is a field the screen and the server can disagree about.
//
// Every duration is an integer count of **minutes** and every amount an integer
// count of **cents**; neither is ever a float and neither is ever summed here.
// `docs/design/projects.md` § "Minutes, and the one place they become hours".

/** Where a week stands. The server's word, never re-derived from timestamps. */
export type WeekStatus = "open" | "submitted" | "approved" | "rejected";

/** The client facts of a project: who it is worked for, at what rate, against
 *  what budget. `null` on a project means internal work — the absence *is* the
 *  answer. */
export interface ProjectClient {
  customerId: string;
  /** ISO 4217, uppercase. Snapshotted from the customer when set. */
  currency: string;
  /** Default hourly rate in integer cents; `null` when nobody has priced it.
   *  Not zero — an unpriced engagement is legal, a free one is a statement. */
  rateCents: number | null;
  /** Budget in hours, held as minutes; `null` when unbudgeted. Advisory. */
  budgetMinutes: number | null;
  /** Budget in money, in integer cents; `null` when unbudgeted. Advisory. */
  budgetCents: number | null;
  /** `YYYY-MM-DD`, or `null`. */
  startsOn: string | null;
  createdAt: string;
  updatedAt: string;
}

/** What a project has cost in hours so far — everybody's, with nobody named.
 *  The one cross-person read the design allows. */
export interface ProjectHours {
  minutes: number;
  billableMinutes: number;
  /** Already carried onto a billing document. */
  billedMinutes: number;
  /** `YYYY-MM-DD` of the last day anybody worked, or `null`. */
  lastWorkedOn: string | null;
  /** Consumption of `client.budgetMinutes` in basis points (10 000 = the whole
   *  budget), or `null` when there is no budget. Over 10 000 is an overrun the
   *  server reports rather than clamps — computed there, never here. */
  budgetConsumptionBp: number | null;
}

/** One engagement: the board Tasks shows, seen as client work. */
export interface Project {
  id: string;
  name: string;
  /** `team` or `personal`. Only a team board can be client work. */
  kind: string;
  color: string | null;
  client: ProjectClient | null;
  hours: ProjectHours;
}

/** The client facts a form sends. A whole record: an omitted field is cleared,
 *  which is what "save" has to mean for a form that shows every field. */
export interface ProjectClientDraft {
  customerId: string;
  currency?: string;
  rateCents?: number | null;
  budgetMinutes?: number | null;
  budgetCents?: number | null;
  startsOn?: string | null;
}

/** The caller's running clock, or `null` when none is. */
export interface RunningTimer {
  projectId: string;
  taskId: string | null;
  /** RFC 3339 instant the clock started. */
  startedAt: string;
  billable: boolean;
  note: string;
}

/** One completed piece of work. */
export interface TimeEntry {
  id: string;
  projectId: string;
  taskId: string | null;
  /** `YYYY-MM-DD` — the day the person says they worked. */
  workDate: string;
  /** RFC 3339 provenance: when a timer or an event produced it, else `null`. */
  startedAt: string | null;
  minutes: number;
  billable: boolean;
  /** The rate snapshotted when the entry was written; `null` when unpriced. */
  rateCents: number | null;
  currency: string | null;
  note: string;
  /** An agent's suggestion, not an hour until a human accepts it. */
  proposed: boolean;
  billed: boolean;
  invoiceId: string | null;
  createdAt: string;
  updatedAt: string;
}

/** Minute totals over a period. Minutes, never money. */
export interface TimeTotals {
  minutes: number;
  billableMinutes: number;
  /** Counted apart: a suggestion invisibly inside a total is not a
   *  suggestion. */
  proposedMinutes: number;
}

/** A person's week and what has been decided about it. */
export interface TimesheetWeek {
  id: string;
  /** `YYYY-MM-DD`, always a Monday. */
  weekStart: string;
  weekEnd: string;
  status: WeekStatus;
  /** True while nothing in the week may be edited. */
  locked: boolean;
  submittedAt: string | null;
  decidedBy: string | null;
  decidedAt: string | null;
  decisionNote: string;
  createdAt: string;
  updatedAt: string;
}

/** A submitted week awaiting a decision, as the approvals inbox reads it. This
 *  is the one shape in the module that names a person, and it reaches only the
 *  admin door. */
export interface PendingWeek extends TimesheetWeek {
  userId: string;
  userEmail: string;
  minutes: number;
  billableMinutes: number;
}

/** What a manual entry, or a correction to one, states. */
export interface TimeEntryDraft {
  projectId?: string;
  taskId?: string | null;
  workDate: string;
  minutes: number;
  billable: boolean;
  note: string;
}
