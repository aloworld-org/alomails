CREATE TABLE meeting_message_reactions (
    tenant_id TEXT NOT NULL,
    meeting_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    emoji TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, meeting_id, message_id, user_id, emoji),
    FOREIGN KEY (tenant_id, meeting_id, message_id)
        REFERENCES meeting_messages (tenant_id, meeting_id, id) ON DELETE CASCADE
);
