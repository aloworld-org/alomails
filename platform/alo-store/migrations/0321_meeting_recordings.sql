CREATE TABLE meeting_recordings (
    tenant_id TEXT NOT NULL,
    meeting_id TEXT NOT NULL,
    id TEXT NOT NULL,
    requested_by TEXT NOT NULL,
    egress_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'recording', 'completed', 'failed')),
    file_path TEXT,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    stopped_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, meeting_id, id),
    FOREIGN KEY (tenant_id, meeting_id)
        REFERENCES meetings (tenant_id, id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX one_active_recording_per_meeting
    ON meeting_recordings (tenant_id, meeting_id)
    WHERE status IN ('pending', 'recording');

CREATE TABLE meeting_recording_consents (
    tenant_id TEXT NOT NULL,
    meeting_id TEXT NOT NULL,
    recording_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    consented_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, meeting_id, recording_id, user_id),
    FOREIGN KEY (tenant_id, meeting_id, recording_id)
        REFERENCES meeting_recordings (tenant_id, meeting_id, id) ON DELETE CASCADE
);
