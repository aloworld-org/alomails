CREATE TABLE project_updates (
    tenant_id  TEXT NOT NULL,
    id         TEXT NOT NULL,
    project_id TEXT NOT NULL,
    state      TEXT NOT NULL CHECK (state IN ('on_track', 'at_risk', 'off_track', 'complete')),
    body       TEXT NOT NULL CHECK (char_length(body) BETWEEN 1 AND 4000),
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, project_id)
        REFERENCES task_projects (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, created_by)
        REFERENCES users (tenant_id, id) ON DELETE RESTRICT
);

CREATE INDEX project_updates_project_recent
    ON project_updates (tenant_id, project_id, created_at DESC, id DESC);
