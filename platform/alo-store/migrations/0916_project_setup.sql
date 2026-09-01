CREATE TABLE project_setup (
    tenant_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    space_id TEXT,
    chat_channel_id TEXT,
    kickoff_event_id TEXT,
    starter_task_ids JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, project_id),
    FOREIGN KEY (tenant_id, project_id)
        REFERENCES task_projects (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT project_setup_task_ids_array CHECK (jsonb_typeof(starter_task_ids) = 'array')
);
