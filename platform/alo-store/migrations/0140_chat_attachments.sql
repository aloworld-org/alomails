-- alo Chat (ADR 0038): sharing a file in a conversation.
--
-- A **pointer to a Drive node, never a copy** — the one-storage law. The
-- precedent is alo Finance's receipt pointer (`receipt_node_id`, ADR 0035) and
-- alo Base's node reference (ADR 0032): the file keeps living in Drive, with
-- one set of permissions, one version history, and one place it can be deleted
-- from. Copying bytes into a chat table would fork all three.
--
-- No FK to drive_nodes. That is deliberate and matches finance: a file may be
-- deleted, trashed, or moved somewhere the reader can no longer reach, and a
-- foreign key would either block that or cascade a message's history away. A
-- pointer that no longer resolves is simply not shown — see
-- `chat_attachments.rs`, which re-resolves every node through Drive's own
-- access check on the way out.
--
-- `position` keeps the order the sharer chose, because "the spec, then the
-- diagram" reads differently from the reverse.

CREATE TABLE chat_attachments (
    tenant_id  TEXT NOT NULL,
    -- Denormalised from the message so a page of the feed is gathered in one
    -- pass, the same reason chat_reactions carries it.
    channel_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    node_id    TEXT NOT NULL,
    position   INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, message_id, node_id),
    FOREIGN KEY (tenant_id, message_id)
        REFERENCES chat_messages (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, channel_id)
        REFERENCES chat_channels (tenant_id, id) ON DELETE CASCADE
);

-- The query a room's feed makes: every attachment on a page, in order.
CREATE INDEX chat_attachments_page
    ON chat_attachments (tenant_id, channel_id, message_id, position);
