-- A durable status update on a project.
--
-- WHY `created_by` IS A SINGLE-COLUMN REFERENCE while `project_id` is a
-- composite one. Composite `(tenant_id, x)` references are this repository's
-- way of making tenancy structural, and `task_projects` supports one because it
-- is keyed `PRIMARY KEY (tenant_id, id)`. **`users` is not**: migration 0001
-- declares it `id TEXT PRIMARY KEY` with only `UNIQUE (tenant_id, email)`
-- beside it, so there is no unique constraint on `(tenant_id, id)` for a
-- composite key to point at. Postgres rejects the reference outright —
--
--     42830: there is no unique constraint matching given keys
--            for referenced table "users"
--
-- — which failed the migration, and a failed migration means EVERY test that
-- builds a schema dies before it starts. Every other table in this schema
-- references `users (id)` singly for the same reason, and that is sufficient:
-- `users.id` is globally unique, so naming it identifies exactly one row. The
-- tenant predicate on reads comes from the store's tenant binding, as it does
-- everywhere else a user is referenced.
CREATE TABLE project_updates (
    tenant_id  TEXT NOT NULL,
    id         TEXT NOT NULL,
    project_id TEXT NOT NULL,
    state      TEXT NOT NULL CHECK (state IN ('on_track', 'at_risk', 'off_track', 'complete')),
    body       TEXT NOT NULL CHECK (char_length(body) BETWEEN 1 AND 4000),
    -- RESTRICT keeps the authorship: a project update says who wrote it, and a
    -- user row cannot be removed while their updates stand.
    created_by TEXT NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, project_id)
        REFERENCES task_projects (tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX project_updates_project_recent
    ON project_updates (tenant_id, project_id, created_at DESC, id DESC);
