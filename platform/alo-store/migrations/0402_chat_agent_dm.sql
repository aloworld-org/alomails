-- A one-to-one with an agent (ADR 0048).
--
-- ADR 0034 says an agent is a first-class chat participant, and chat had two
-- room shapes, neither of which holds one. A DM is identified by `dm_key`,
-- documented in 0132 as "both member ids sorted and joined" — two **user** ids
-- — and 0141 deliberately refuses to make an agent a row in `users`. So the
-- shape every chat user reaches for first, open a conversation with the thing
-- you want to talk to, could not be expressed without putting a non-user id in
-- a column every reader treats as a person.
--
-- A third kind instead. `dm_key` keeps its single meaning, and old code paths
-- that switch on `kind` **refuse** an `agent_dm` rather than misreading it as a
-- DM between two humans.
--
-- Expand-only: one nullable column, one new index, and two CHECKs replaced by
-- strictly more permissive ones — every row that satisfied the old constraint
-- satisfies the new one, so nothing existing is touched or rewritten.

ALTER TABLE chat_channels
    -- Whose one-to-one this is. NULL for every other kind; the shape CHECK
    -- below is what keeps it that way. MATCH SIMPLE means a NULL here skips
    -- the foreign key entirely, which is exactly right for the two kinds that
    -- have no agent.
    ADD COLUMN agent_id TEXT,
    ADD CONSTRAINT chat_channels_agent
        FOREIGN KEY (tenant_id, agent_id)
        REFERENCES chat_agents (tenant_id, id) ON DELETE CASCADE;

ALTER TABLE chat_channels DROP CONSTRAINT chat_channels_kind;
ALTER TABLE chat_channels
    ADD CONSTRAINT chat_channels_kind
        CHECK (kind IN ('channel', 'dm', 'agent_dm'));

-- The three shapes are exclusive: a named room has a name and nothing else; a
-- DM has a dm_key; an agent DM has an agent and, like a DM, no name and no
-- other way in.
ALTER TABLE chat_channels DROP CONSTRAINT chat_channels_shape;
ALTER TABLE chat_channels
    ADD CONSTRAINT chat_channels_shape CHECK (
        (kind = 'channel' AND name IS NOT NULL AND dm_key IS NULL AND agent_id IS NULL)
        OR (kind = 'dm' AND name IS NULL AND dm_key IS NOT NULL
            AND visibility = 'private' AND agent_id IS NULL)
        OR (kind = 'agent_dm' AND name IS NULL AND dm_key IS NULL
            AND visibility = 'private' AND agent_id IS NOT NULL)
    );

-- One room per person per agent, per tenant — the same idempotency `dm_key`
-- gives a human DM, enforced by the database so two simultaneous opens cannot
-- make two rooms. `created_by` is the human: an agent DM has exactly one, and
-- it is the person who opened it.
CREATE UNIQUE INDEX chat_channels_agent_dm
    ON chat_channels (tenant_id, agent_id, created_by)
    WHERE kind = 'agent_dm';
