CREATE TABLE meeting_message_attachments (
    tenant_id TEXT NOT NULL,
    meeting_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    id TEXT NOT NULL,
    file_name TEXT NOT NULL,
    content_type TEXT NOT NULL,
    data BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, meeting_id, message_id, id),
    FOREIGN KEY (tenant_id, meeting_id, message_id)
        REFERENCES meeting_messages (tenant_id, meeting_id, id) ON DELETE CASCADE
);
