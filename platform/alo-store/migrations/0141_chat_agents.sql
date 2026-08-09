-- alo Chat: agents as participants (ADR 0034 §chat, ADR 0038;
-- docs/design/chat-agents.md).
--
-- An agent has an IDENTITY of its own but no AUTHORITY of its own. It posts
-- under its own name; it reads and acts through the account door of whoever
-- asked it. That separation is the whole design, and the schema is built to
-- make the wrong thing hard: there is no credential here, no api key, nothing
-- an agent could authenticate as. It cannot act, only be named.
--
-- Deliberately NOT a row in `users`: that would make an agent mailable,
-- assignable, addressable in Spaces and countable as a seat, and would put a
-- non-human in a table whose every consumer assumes a person.

CREATE TABLE chat_agents (
    tenant_id   TEXT NOT NULL,
    id          TEXT NOT NULL,
    -- What people type after '@'. Lowercase, no '@', unique per tenant.
    handle      TEXT NOT NULL,
    name        TEXT NOT NULL,
    description TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Switched off rather than deleted: its past messages must keep their
    -- author, and a room's history must not change because an agent was
    -- retired.
    disabled_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, id)
);

-- One '@handle' per tenant, so a mention is never ambiguous.
CREATE UNIQUE INDEX chat_agents_handle
    ON chat_agents (tenant_id, lower(handle));

-- Who said it, and on whose behalf.
--
-- `author_id` already carries no foreign key to `users`, so an agent id lives
-- there without schema violence. A reader tells a person from an agent by
-- `author_kind` and never by parsing an id.
ALTER TABLE chat_messages
    ADD COLUMN author_kind TEXT NOT NULL DEFAULT 'user',
    -- The asker, on an agent's message. The room sees the agent; the record
    -- shows whose reach produced it. Both are true and neither is hidden.
    ADD COLUMN on_behalf_of TEXT;

ALTER TABLE chat_messages
    ADD CONSTRAINT chat_messages_author_kind
        CHECK (author_kind IN ('user', 'agent')),
    -- An agent message always names its asker; a person's message never does.
    ADD CONSTRAINT chat_messages_on_behalf
        CHECK (
            (author_kind = 'agent' AND on_behalf_of IS NOT NULL)
            OR (author_kind = 'user' AND on_behalf_of IS NULL)
        );

-- An agent belongs to rooms, the way a person does.
CREATE TABLE chat_agent_members (
    tenant_id  TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    agent_id   TEXT NOT NULL,
    added_by   TEXT NOT NULL,
    added_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, channel_id, agent_id),
    FOREIGN KEY (tenant_id, channel_id)
        REFERENCES chat_channels (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, agent_id)
        REFERENCES chat_agents (tenant_id, id) ON DELETE CASCADE
);

-- A proposed action, waiting for a tap (ADR 0023, ADR 0034).
--
-- This is a table and not client state because a chat proposal is seen by a
-- room, must survive a reload, must be refusable, and must leave a record of
-- who decided. The existing command-palette flow keeps proposals in React
-- state, which is enough for one person for four seconds and not enough here.
CREATE TABLE chat_proposals (
    tenant_id  TEXT NOT NULL,
    id         TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    -- The agent's message carrying it, so it renders in place.
    message_id TEXT NOT NULL,
    -- The person whose words caused it. ONLY THEY MAY APPROVE: the proposal
    -- was computed through their access, so approving it as anyone else would
    -- run their reach on another person's say-so.
    asked_by   TEXT NOT NULL,
    tool       TEXT NOT NULL,
    args       JSONB NOT NULL,
    state      TEXT NOT NULL DEFAULT 'pending',
    decided_by TEXT,
    decided_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, message_id)
        REFERENCES chat_messages (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, channel_id)
        REFERENCES chat_channels (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT chat_proposals_state
        CHECK (state IN ('pending', 'approved', 'discarded', 'expired')),
    -- Decided means decided by somebody, at some point; pending means neither.
    CONSTRAINT chat_proposals_decided
        CHECK (
            (state = 'pending' AND decided_by IS NULL AND decided_at IS NULL)
            OR (state <> 'pending' AND decided_by IS NOT NULL AND decided_at IS NOT NULL)
        )
);

-- Drawing a page of the feed: the proposals on those messages.
CREATE INDEX chat_proposals_page
    ON chat_proposals (tenant_id, channel_id, message_id);
