-- The tenant's event stream (ADR 0058 §5): every intent execution leaves one
-- row here, so that what happened in a workspace is a thing consumers read
-- rather than something each of them re-derives by polling record tables.
-- Audit is the first consumer (a record's history shows what agents did to
-- it); notifications, standing instructions and memory extraction follow.
--
-- **Append-only.** The store exposes no update and no delete; a row is wrong
-- only if it never happened, and then the bug is at the emitter.
--
-- What is NOT here, deliberately: the intent's arguments and its result.
-- An event says that a verb ran and which record it touched; the record
-- itself stays in its module's tables, and the arguments live in
-- `agent_tool_runs` where the run's own audit already keeps them.
-- Constitution law #1: no second store of content.

CREATE TABLE events (
    tenant_id     TEXT NOT NULL,
    id            TEXT NOT NULL,
    -- The verb that ran, as the registry names it (`send_quote`); the route
    -- path's vocabulary joins in wave A8 when a person's click becomes the
    -- same action object as an agent's proposal.
    kind          TEXT NOT NULL,
    -- The record the execution touched, when it touched exactly one: the
    -- record word the executor's own reply uses (`quote`, `invoice`) and the
    -- record's id. NULL for an execution about no single record (a list read,
    -- a totals question).
    record_type   TEXT,
    record_id     TEXT,
    -- The person whose access the execution ran through — never an agent's,
    -- because an agent has none (see 0141).
    actor_user_id TEXT NOT NULL,
    -- The agent that ran it, when one did. NULL for a person's own tap in the
    -- command palette.
    agent_id      TEXT,
    -- 'read' ran inside a turn; 'write' ran from an approval. Stored rather
    -- than derived so the stream stays true if a verb's effect is ever
    -- re-declared.
    effect        TEXT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    CONSTRAINT events_effect CHECK (effect IN ('read', 'write'))
);

-- The two questions asked of the stream so far: what happened to this record
-- (the audit tab), and what have I run lately (the caller's own history).
CREATE INDEX events_by_record
    ON events (tenant_id, record_id, created_at DESC)
    WHERE record_id IS NOT NULL;
CREATE INDEX events_by_actor
    ON events (tenant_id, actor_user_id, created_at DESC);
