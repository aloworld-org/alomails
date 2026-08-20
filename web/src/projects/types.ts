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
  ownerId: string;
  description: string | null;
  status: "planned" | "active" | "on_hold" | "completed" | "cancelled";
  startsOn: string | null;
  targetOn: string | null;
  createdAt: string;
  updatedAt: string;
  client: ProjectClient | null;
  hours: ProjectHours;
}

export interface ProjectDraft {
  name: string;
  description: string | null;
  status: Project["status"];
  startsOn: string | null;
  targetOn: string | null;
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

/** What one currency's rated hours are worth over the report's period. The
 *  server folds every one of these figures; none is summed here. */
export interface ProfitabilityCurrency {
  /** ISO 4217 — the currency the hours were **priced in**, which is not always
   *  the engagement's own. */
  currency: string;
  billableMinutes: number;
  /** Their value net of VAT, in integer cents. */
  netCents: number;
  billedMinutes: number;
  /** The part already carried onto a document. */
  billedNetCents: number;
  /** `netCents - billedNetCents`, subtracted by the server: what is earned and
   *  still to invoice. */
  unbilledNetCents: number;
}

/** One engagement's profitability: what the period produced, and how much of
 *  the budget has gone. */
export interface ProjectProfitability {
  projectId: string;
  projectName: string;
  customerId: string;
  /** The engagement's own currency — the one its money budget is stated in. */
  currency: string;
  budgetMinutes: number | null;
  budgetCents: number | null;
  /** Every accepted minute inside the period, billable or not. */
  minutes: number;
  billableMinutes: number;
  /** Chargeable minutes carrying no rate: counted, and priced nowhere. */
  unratedMinutes: number;
  /** One row per currency the period's rated hours were priced in. Never added
   *  together — this report does not convert. */
  byCurrency: ProfitabilityCurrency[];
  /** Everything up to and including the period's last day — what a budget is
   *  consumed by. */
  toDateMinutes: number;
  /** The value of the rated hours to date, in the engagement's own currency. */
  toDateNetCents: number;
  /** Consumption in basis points (10 000 = the whole budget), or `null` when
   *  there is no budget. Over 10 000 is an overrun the server reports rather
   *  than clamps. */
  hoursConsumptionBp: number | null;
  budgetConsumptionBp: number | null;
  /** What is left of the money budget; negative past it. `null` without one. */
  budgetRemainingCents: number | null;
}

/** What a whole report adds up to: minutes across every engagement, and value
 *  one row per currency. */
export interface ProfitabilityTotals {
  minutes: number;
  billableMinutes: number;
  unratedMinutes: number;
  byCurrency: ProfitabilityCurrency[];
}

/** The profitability report for a stated period. */
export interface ProfitabilityReport {
  /** `YYYY-MM-DD`, inclusive. */
  from: string;
  to: string;
  projects: ProjectProfitability[];
  totals: ProfitabilityTotals;
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

/** One milestone: a named date on a project, with what the timeline draws
 *  beside it. Every field is the server's — including `late`, which is judged
 *  against the server's date so a browser with a wrong clock cannot clear its
 *  own late list. */
export interface Milestone {
  id: string;
  projectId: string;
  name: string;
  /** `YYYY-MM-DD`. A milestone always has one. */
  dueOn: string;
  /** Whether a human marked it reached. Never derived from its tasks. */
  done: boolean;
  /** RFC 3339 instant it was reached, or `null`. */
  doneAt: string | null;
  /** Past its day and not reached, as at the server's today. */
  late: boolean;
  /** How many tasks are placed under it, and how many of those are closed.
   *  Information beside `done`, never the thing itself. */
  taskCount: number;
  taskDoneCount: number;
  createdAt: string;
  updatedAt: string;
}

/** Where one task sits in the plan. One milestone per task. */
export interface TaskPlacement {
  taskId: string;
  milestoneId: string;
}

/** One project's plan and the placements over it — the timeline's single
 *  read. */
export interface ProjectPlan {
  milestones: Milestone[];
  placements: TaskPlacement[];
}

/** What a milestone form states: the two facts a milestone is. */
export interface MilestoneDraft {
  name: string;
  /** `YYYY-MM-DD`. */
  dueOn: string;
}

/** A board somebody marked reusable. A template IS a project, so it is named by
 *  the board's own id — there is no second record to keep in step. */
export interface ProjectTemplate {
  projectId: string;
  name: string;
  color: string | null;
  /** Whether the board itself has been archived. Archiving a template is an
   *  ordinary way to keep a shape without keeping it in the board list, so it
   *  is still listed and still usable. */
  archived: boolean;
  /** How many cards a copy would carry — open work only, so the number here is
   *  the number that will appear on the new board. */
  taskCount: number;
  milestoneCount: number;
  createdBy: string;
  createdAt: string;
}

/** What the create-from-template form states. */
export interface TemplateInstanceDraft {
  name: string;
  /** `YYYY-MM-DD`, or `null` to copy every date as it stands. The template's
   *  earliest milestone lands on this day and the rest keep their spacing. */
  startsOn: string | null;
  /** The customer the new engagement is for, or `null` for internal work. The
   *  template's own customer is never copied — a template is a shape, not a
   *  client. */
  customerId: string | null;
}

/** What one copy produced: the new board, and what landed on it. */
export interface TemplateCopy {
  projectId: string;
  taskCount: number;
  milestoneCount: number;
}
