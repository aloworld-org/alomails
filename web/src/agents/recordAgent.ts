// What a record's agent can start from the record itself (A8.4).
//
// The panel never runs anything: a verb here is a button that opens the
// agent's one-to-one with the words pre-filled, and the agent then proposes —
// the person still sends, and a write still waits for their one approval
// (ADR 0023). So this catalogue holds exactly two things per verb: the words
// on the button and the words in the composer.
//
// The directory (`GET /chat/agents/directory`) is the authority on what an
// agent may do — its `tools` are rendered from the intent registry. This
// catalogue only says which of those verbs take one record and how to say
// them; the panel shows the intersection, so a verb the boundary would refuse
// is never offered.
import { strings } from "../i18n";

/** Where a record came from, as its record view carries it: the same
 *  `{kind, id, label}` shape provenance is stored under (ADR 0058 §4). */
export interface RecordOrigin {
  /** The event stream's word for the source — `thread`, `message`, `event`,
   *  `quote`, `person`, … */
  kind: string;
  id: string;
  /** A citation when the source has a name; `null` when it has none. */
  label: string | null;
}

/** One verb of a product that takes the record in focus. */
export interface RecordVerb {
  /** The registry's stable id — matched against the directory's `tools`. */
  tool: string;
  /** The button's words. A function, so the active locale is read at
   *  render time like every other string. */
  label: () => string;
  /** The composer pre-fill: the ask in the user's own language, with the
   *  record named — sent by the person, never by the panel. */
  draft: (recordLabel: string) => string;
  /** The record kinds this verb takes, for a product whose detail views show
   *  more than one (an expense is approved, a bank line is categorised).
   *  Absent means every kind — the single-record products say nothing. */
  kinds?: readonly string[];
}

/** The verbs of `product` that take a record of `recordKind`. */
export function verbsFor(
  product: string,
  recordKind: string,
): readonly RecordVerb[] {
  return (RECORD_VERBS[product] ?? []).filter(
    (verb) => verb.kinds === undefined || verb.kinds.includes(recordKind),
  );
}

/** Per product, the verbs that take one record — the module's word for
 *  itself (`tasks`, `billing`, …) keyed exactly as the directory's
 *  `product` field spells it. A product not listed yet simply offers no
 *  verb buttons; its panel still shows origin and ask. */
export const RECORD_VERBS: Readonly<Record<string, readonly RecordVerb[]>> = {
  tasks: [
    {
      tool: "chase_task",
      label: () => strings.recordAgentVerbChaseTask,
      draft: (task) => strings.recordAgentDraftChaseTask(task),
    },
    {
      tool: "set_task_priority",
      label: () => strings.recordAgentVerbSetTaskPriority,
      draft: (task) => strings.recordAgentDraftSetTaskPriority(task),
    },
    {
      tool: "complete_task",
      label: () => strings.recordAgentVerbCompleteTask,
      draft: (task) => strings.recordAgentDraftCompleteTask(task),
    },
    {
      tool: "reassign_task",
      label: () => strings.recordAgentVerbReassignTask,
      draft: (task) => strings.recordAgentDraftReassignTask(task),
    },
  ],
  crm: [
    {
      tool: "move_deal_stage",
      label: () => strings.recordAgentVerbMoveDealStage,
      draft: (deal) => strings.recordAgentDraftMoveDealStage(deal),
    },
    {
      tool: "draft_followup",
      label: () => strings.recordAgentVerbDraftFollowup,
      draft: (deal) => strings.recordAgentDraftDraftFollowup(deal),
    },
  ],
  finance: [
    // Approving is the approver's act on a waiting claim, so the verb rides
    // the approvals queue's record — named by merchant, exactly as
    // `approve_expense` asks for it — and not the claimant's own editor.
    {
      tool: "approve_expense",
      label: () => strings.recordAgentVerbApproveExpense,
      draft: (merchant) => strings.recordAgentDraftApproveExpense(merchant),
      kinds: ["approval"],
    },
    // `categorise_transactions` reads the asker's own uncategorised claims
    // over a period — the record only prompts it, so the draft names none.
    {
      tool: "categorise_transactions",
      label: () => strings.recordAgentVerbSuggestCategories,
      draft: () => strings.recordAgentDraftSuggestCategories,
      kinds: ["expense"],
    },
  ],
  projects: [
    {
      tool: "project_status_summary",
      label: () => strings.recordAgentVerbProjectStatus,
      draft: (project) => strings.recordAgentDraftProjectStatus(project),
      kinds: ["project"],
    },
    {
      tool: "log_time",
      label: () => strings.recordAgentVerbLogTime,
      draft: (project) => strings.recordAgentDraftLogTime(project),
      kinds: ["project"],
    },
    {
      tool: "draft_timesheet_from_calendar",
      label: () => strings.recordAgentVerbDraftTimesheet,
      draft: (week) => strings.recordAgentDraftDraftTimesheet(week),
      kinds: ["timesheet"],
    },
  ],
  inventory: [
    {
      tool: "receive_delivery",
      label: () => strings.recordAgentVerbReceiveDelivery,
      draft: (order) => strings.recordAgentDraftReceiveDelivery(order),
      kinds: ["purchaseOrder"],
    },
  ],
  hr: [
    {
      tool: "approve_leave_request",
      label: () => strings.recordAgentVerbApproveLeave,
      draft: (leave) => strings.recordAgentDraftApproveLeave(leave),
      kinds: ["leave"],
    },
    {
      tool: "draft_letter_from_template",
      label: () => strings.recordAgentVerbDraftLetter,
      draft: (person) => strings.recordAgentDraftDraftLetter(person),
      kinds: ["applicant", "person"],
    },
  ],
};
