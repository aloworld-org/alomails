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
};
