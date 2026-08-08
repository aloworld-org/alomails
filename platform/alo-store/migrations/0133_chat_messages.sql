-- alo Chat (ADR 0038), phase 3: what people actually say.
--
-- Ordering is a **per-channel monotonic sequence**, not a timestamp — the
-- pattern mailbox UIDs and gapless invoice numbers already use here. It makes
-- pagination exact under equal timestamps and clock skew, makes read state one
-- integer, and makes sync idempotent. The counter lives on the channel row and
-- is allocated inside the posting transaction (mailboxes.uid_next precedent).

ALTER TABLE chat_channels ADD COLUMN next_seq BIGINT NOT NULL DEFAULT 1;

CREATE TABLE chat_messages (
    tenant_id       TEXT NOT NULL,
    channel_id      TEXT NOT NULL,
    id              TEXT NOT NULL,
    -- Position in the room, 1-based, never reused and never a hole: a deleted
    -- message keeps its row (and its seq) as a tombstone.
    seq             BIGINT NOT NULL,
    author_id       TEXT NOT NULL,
    body            TEXT NOT NULL,
    -- 'text' (a person spoke) | 'system' (the room narrating itself).
    kind            TEXT NOT NULL DEFAULT 'text',
    -- The seq of the message this one replies to; NULL for the main feed.
    -- One level only: a reply is never itself a thread root.
    thread_root_seq BIGINT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    edited_at       TIMESTAMPTZ,
    -- Set when withdrawn; the body is emptied at the same moment, so a deleted
    -- message leaves ordering intact without leaving its content behind.
    deleted_at      TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, channel_id)
        REFERENCES chat_channels (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT chat_messages_kind CHECK (kind IN ('text', 'system'))
);

-- The sequence is the room's clock: one message per position, enforced.
CREATE UNIQUE INDEX chat_messages_seq
    ON chat_messages (tenant_id, channel_id, seq);

-- The query every open room makes: the newest page, walking backwards.
CREATE INDEX chat_messages_history
    ON chat_messages (tenant_id, channel_id, seq DESC);

-- A thread's replies, gathered under their root.
CREATE INDEX chat_messages_thread
    ON chat_messages (tenant_id, channel_id, thread_root_seq)
    WHERE thread_root_seq IS NOT NULL;
