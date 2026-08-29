-- Goals (ADR 0058 §7, `docs/design/complete-agents.md` §8, queue item A8.3):
-- multi-step work across agents is an object — the plan Ask alo made, its
-- progress, one approval surface, Stop — not a conversation between agents.
--
-- Before this the plan lived only in a room message: the run stopped at the
-- first write and "the rest of this waits until you approve that" was a
-- sentence with nothing behind it — the remaining steps were simply dropped.
-- This row is what the approval resumes.

CREATE TABLE agent_goals (
    tenant_id   TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    id          TEXT NOT NULL,
    channel_id  TEXT NOT NULL,
    -- The Ask alo agent that planned it; the resumed run speaks as it.
    agent_id    TEXT NOT NULL,
    -- Whose reach every step runs at. Only they may approve the goal's
    -- proposals (the proposal table already enforces that), and only they may
    -- move or end the goal.
    asked_by    TEXT NOT NULL,
    -- The goal in the asker's own words, exactly as the plan was made from.
    request     TEXT NOT NULL,
    -- The plan, fixed at creation: [{"agent": handle, "ask": text}, …].
    -- Progress lives in `cursor`, never by editing this — the card must show
    -- the plan that was announced, not one that drifted while it ran.
    steps       JSONB NOT NULL,
    -- Steps before this index are done. cursor == step count means finished.
    cursor      INTEGER NOT NULL DEFAULT 0,
    status      TEXT NOT NULL DEFAULT 'working',
    -- The pending proposal the goal is waiting behind — its one approval
    -- surface. Set exactly while waiting, cleared on every way out of it.
    proposal_id TEXT,
    -- Why it ended, when it ended early. NULL for done and for stopped-by-hand.
    note        TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    CONSTRAINT agent_goals_status CHECK
        (status IN ('working', 'waiting', 'done', 'stopped', 'failed')),
    -- Waiting means waiting on something: the two facts cannot disagree.
    CONSTRAINT agent_goals_waiting_shape CHECK
        ((status = 'waiting') = (proposal_id IS NOT NULL))
);

-- The room's goal card lists a channel's goals.
CREATE INDEX agent_goals_channel ON agent_goals (tenant_id, channel_id);

-- A settled proposal asks "was a goal waiting on me" — one lookup, not a scan.
CREATE INDEX agent_goals_proposal ON agent_goals (tenant_id, proposal_id)
    WHERE proposal_id IS NOT NULL;
