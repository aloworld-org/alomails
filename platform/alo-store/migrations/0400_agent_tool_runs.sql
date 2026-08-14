-- Every tool an agent has run, read or write (ADR 0047 §4).
--
-- Until now an agent's record was counted from `chat_proposals`: a tool ran
-- only because somebody approved it, so the approvals *were* the history.
-- ADR 0047 lets a reading tool run inside the turn without a tap, and eleven
-- of the thirty-three tools are reads. Without this table, those eleven would
-- run leaving nothing behind, and the one surface that shows what an agent has
-- done would quietly stop showing a third of what it did.
--
-- So both paths are recorded here, not just the new one. A single log answers
-- "what has this agent touched, on whose behalf, and when" without a reader
-- having to join two half-histories and know which is which.
--
-- What is NOT here, deliberately: the tool's RESULT. Arguments name what was
-- asked for; a result is the record itself — a diary, a message body, a stock
-- figure — and copying it into an audit table would make a second store of
-- content with its own access rules to get wrong. Constitution law #1.

CREATE TABLE agent_tool_runs (
    tenant_id  TEXT NOT NULL,
    id         TEXT NOT NULL,
    -- The agent that ran it. NULL for the workspace assistant reached from
    -- the command palette, which is not a row in `chat_agents`.
    agent_id   TEXT,
    -- The room it happened in. NULL outside chat, same reason.
    channel_id TEXT,
    -- The person whose access it ran through — never the agent's, because an
    -- agent has none (see 0141).
    asked_by   TEXT NOT NULL,
    tool       TEXT NOT NULL,
    -- 'read' ran inside the turn; 'write' ran from an approval. Stored rather
    -- than derived, so the history stays true if a tool's effect is ever
    -- re-declared.
    effect     TEXT NOT NULL,
    args       JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- Whether it actually did what it was asked. A refused write and a failed
    -- read are both worth keeping: an audit that records only successes is an
    -- audit that hides exactly the interesting rows.
    ok         BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    CONSTRAINT agent_tool_runs_effect CHECK (effect IN ('read', 'write'))
);

-- The two questions asked of it: what has this agent done, and what has
-- happened in this room.
CREATE INDEX agent_tool_runs_by_agent
    ON agent_tool_runs (tenant_id, agent_id, created_at DESC);
CREATE INDEX agent_tool_runs_by_channel
    ON agent_tool_runs (tenant_id, channel_id, created_at DESC);
