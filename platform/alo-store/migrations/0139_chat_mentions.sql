-- alo Chat (ADR 0038): naming someone in a message.
--
-- Written at post time rather than found by scanning text later. "Does this
-- room have something for me?" is asked on every sidebar draw, and answering
-- it by searching bodies would put a text scan on the hot path of every
-- screen. Resolving once, when the words are written, turns it into an index
-- lookup.
--
-- `seq` is denormalised from the message so an unread-mention count is a
-- comparison against the reader's cursor without joining back to
-- chat_messages.
--
-- Only members can be mentioned: an @ that resolves to nobody in the room
-- stays plain text. That is deliberate — a mention that reached someone who
-- cannot open the room would be a notification to a place they cannot go.

CREATE TABLE chat_mentions (
    tenant_id  TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    seq        BIGINT NOT NULL,
    user_id    TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, message_id, user_id),
    FOREIGN KEY (tenant_id, message_id)
        REFERENCES chat_messages (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, channel_id)
        REFERENCES chat_channels (tenant_id, id) ON DELETE CASCADE
);

-- "What is waiting for me here?" — one index serves both the per-room badge
-- and the count across every room.
CREATE INDEX chat_mentions_for_user
    ON chat_mentions (tenant_id, user_id, channel_id, seq);
