-- Channel memory for agents (ADR 0057 §6, `docs/design/complete-agents.md`,
-- queue item A6.1). **The channel is the consent boundary**: what was shared
-- in a channel may be remembered by the agents in it and used there and
-- nowhere else; a one-to-one with an agent feeds only that person's memory.
--
-- Facts and decisions with the message they came from — never transcripts.
-- The store's cap and length limits enforce the second half of that sentence;
-- this schema enforces the first: a row is one fact, one agent, one scope.

CREATE TABLE agent_memories (
    tenant_id    TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    id           TEXT NOT NULL,
    agent_id     TEXT NOT NULL,
    -- 'channel' (learned in a room, usable in that room) | 'person' (learned
    -- in a one-to-one with the agent, usable only for that person).
    scope        TEXT NOT NULL,
    channel_id   TEXT,
    user_id      TEXT,
    -- One short standalone fact, in words. Never a transcript; the store
    -- refuses anything long enough to be one.
    fact         TEXT NOT NULL,
    -- The message it came from, when there is one — what A6.3's "deletion
    -- follows the source" will follow, and what "What I remember" cites.
    source_msg   TEXT,
    -- 'turn' (extracted at the end of a turn from what the turn read) |
    -- 'explicit' (a person said "remember that …", which works even where
    -- learning is switched off).
    learned_from TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    CONSTRAINT agent_memories_scope CHECK (scope IN ('channel', 'person')),
    CONSTRAINT agent_memories_learned CHECK (learned_from IN ('turn', 'explicit')),
    -- The two scopes are exclusive: a channel memory names its room and no
    -- person; a person memory names its person and no room. There is no
    -- cross-channel pool to fall into by leaving both NULL.
    CONSTRAINT agent_memories_shape CHECK (
        (scope = 'channel' AND channel_id IS NOT NULL AND user_id IS NULL)
        OR (scope = 'person' AND user_id IS NOT NULL AND channel_id IS NULL)
    )
);

-- Retrieval is always "this agent, this room" or "this agent, this person" —
-- the only two doors A6.2 will read through.
CREATE INDEX agent_memories_channel
    ON agent_memories (tenant_id, agent_id, channel_id)
    WHERE scope = 'channel';
CREATE INDEX agent_memories_person
    ON agent_memories (tenant_id, agent_id, user_id)
    WHERE scope = 'person';

-- The per-channel switch (room settings). NULL means "follow the workspace
-- default", so an admin flipping the default moves every room that never
-- chose for itself.
ALTER TABLE chat_channels ADD COLUMN agent_memory BOOLEAN;

-- The workspace default (admin console). No row means ON — memory is on by
-- default, and a tenant that has never touched the switch has no row.
CREATE TABLE agent_memory_defaults (
    tenant_id  TEXT PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    enabled    BOOLEAN NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
