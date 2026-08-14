CREATE TABLE meeting_messages (
    tenant_id TEXT NOT NULL,
    meeting_id TEXT NOT NULL,
    id TEXT NOT NULL,
    sender_id TEXT NOT NULL,
    recipient_id TEXT,
    body TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, meeting_id, id),
    FOREIGN KEY (tenant_id, meeting_id) REFERENCES meetings (tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX meeting_messages_visible_idx
    ON meeting_messages (tenant_id, meeting_id, created_at, id);
