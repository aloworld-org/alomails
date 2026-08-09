-- alo Chat (ADR 0038): reacting to a message.
--
-- A reaction is a fact about one person and one message, so the row IS the
-- key: (message, user, emoji). Reacting twice with the same emoji is not a
-- second reaction, it is a toggle back off — the primary key enforces that,
-- rather than the application counting and hoping.
--
-- No count is stored. A tally is cheap to derive and impossible to get wrong;
-- a stored counter is a second source of truth that drifts the first time a
-- delete races an insert.
--
-- The permitted emoji live in the store (`REACTIONS`), not in a CHECK
-- constraint. The set will grow, and growing it should not require a
-- migration on every tenant's database.

CREATE TABLE chat_reactions (
    tenant_id  TEXT NOT NULL,
    -- Denormalised from the message so a page of the feed can be tallied
    -- without joining back through chat_messages.
    channel_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    user_id    TEXT NOT NULL,
    emoji      TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, message_id, user_id, emoji),
    FOREIGN KEY (tenant_id, message_id)
        REFERENCES chat_messages (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, channel_id)
        REFERENCES chat_channels (tenant_id, id) ON DELETE CASCADE
);

-- The query every open room makes: tally every reaction on a page of messages
-- in one pass.
CREATE INDEX chat_reactions_page
    ON chat_reactions (tenant_id, channel_id, message_id);
