-- A task assigned to an agent is a standing instruction with a due date
-- (ADR 0058 §6, queue item A8.2): a third trigger kind, 'once', that fires
-- at its moment and then leaves — claimed by deletion, so it cannot fire
-- twice and a fired assignment is not a card forever saying "paused".
--
-- Expand-only: the two constraints are re-stated to admit the new kind; no
-- row that satisfied the old constraints violates the new ones.

ALTER TABLE agent_instructions DROP CONSTRAINT agent_instructions_trigger;
ALTER TABLE agent_instructions ADD CONSTRAINT agent_instructions_trigger
    CHECK (trigger_kind IN ('schedule', 'event', 'once'));

ALTER TABLE agent_instructions DROP CONSTRAINT agent_instructions_shape;
ALTER TABLE agent_instructions ADD CONSTRAINT agent_instructions_shape CHECK (
    (trigger_kind = 'schedule' AND repeat_minutes IS NOT NULL
         AND next_run IS NOT NULL AND event_kind IS NULL)
    OR (trigger_kind = 'event' AND event_kind IS NOT NULL
         AND repeat_minutes IS NULL AND next_run IS NULL)
    -- 'once': its moment and nothing else — no repeat (it does not), no
    -- event (a clock, not the stream).
    OR (trigger_kind = 'once' AND next_run IS NOT NULL
         AND repeat_minutes IS NULL AND event_kind IS NULL)
);

-- The sweep's due scan for one-shot firings, beside the schedules' own.
CREATE INDEX agent_instructions_once_due ON agent_instructions (next_run)
    WHERE trigger_kind = 'once' AND paused_at IS NULL;
