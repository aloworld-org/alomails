CREATE TABLE meeting_transcript_segments (
    tenant_id TEXT NOT NULL,
    meeting_id TEXT NOT NULL,
    id TEXT NOT NULL,
    speaker_id TEXT NOT NULL,
    text TEXT NOT NULL,
    final BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, meeting_id, id),
    FOREIGN KEY (tenant_id, meeting_id)
        REFERENCES meetings (tenant_id, id) ON DELETE CASCADE
);
