-- The action record (ADR 0058 §6, `docs/design/complete-agents.md` §4,
-- queue item A8.1): a person's click and an agent's proposal are the same
-- object.
--
-- `agent_tool_runs` already keeps exactly one row per intent execution,
-- whoever asked — an agent's read inside a turn, an agent's write from an
-- approved proposal, a person's own tap in the command palette. This grows
-- that row into the action record the design names, rather than starting a
-- third log beside it and `events`: one log, not two half-histories (0400).
--
-- Who acted is already on the row: `asked_by` is the person whose access the
-- execution ran through (the design's on_behalf_of), and `agent_id` is the
-- agent that acted when one did (the actor); a NULL agent means the person
-- acted for themselves.
--
-- What is still NOT here: the execution's result payload. The result is the
-- record itself, and it stays in its module's tables (constitution law #1,
-- see 0400). The result the action record keeps is a *pointer* — the record
-- the execution touched — which is also what an undo needs to name.

-- For a write with a preview template: the sentence shown before anyone taps,
-- rendered with the resolved arguments — what this would do. NULL for reads,
-- and for verbs whose registry entry declares no preview.
ALTER TABLE agent_tool_runs ADD COLUMN preview TEXT;

-- The record the execution touched, when it touched exactly one: the record
-- word the executor's own reply uses (`quote`, `invoice`, `payment`) and the
-- record's id. Same vocabulary as `events` (0406).
ALTER TABLE agent_tool_runs ADD COLUMN record_type TEXT;
ALTER TABLE agent_tool_runs ADD COLUMN record_id   TEXT;

-- The inverse verb and the arguments that would undo THIS run, kept only when
-- the registry declares an inverse and the run succeeded touching a record to
-- point it at. NULL means the domain has no inverse ("an issued invoice
-- cannot be un-issued") — not that nobody has written one yet.
ALTER TABLE agent_tool_runs ADD COLUMN undo_tool TEXT;
ALTER TABLE agent_tool_runs ADD COLUMN undo_args JSONB;

-- The `chat_proposals` row this execution settled, when it came from one.
-- This is the sentence "a person's click and an agent's proposal are the same
-- object" made a join: the proposal card and the palette tap both end as a
-- row here, and this column says which card an execution grew out of. No
-- foreign key, matching `agent_id`/`channel_id` above: the action record
-- outlives what it references.
ALTER TABLE agent_tool_runs ADD COLUMN proposal_id TEXT;
