CREATE TABLE meeting_workspaces (
    tenant_id TEXT NOT NULL,
    meeting_id TEXT NOT NULL,
    state JSONB NOT NULL DEFAULT '{"agenda":[],"polls":[],"notes":""}'::jsonb,
    revision BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, meeting_id),
    FOREIGN KEY (tenant_id, meeting_id)
        REFERENCES meetings (tenant_id, id) ON DELETE CASCADE
);
