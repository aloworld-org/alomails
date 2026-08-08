-- alo Projects (ADR 0035, wave B3.09): project templates — the mark that says
-- "this board is a reusable engagement shape", and nothing else.
--
-- A TEMPLATE IS A PROJECT. This table holds one row per project a tenant has
-- marked reusable; the shape itself lives in `task_projects`, `tasks`,
-- `task_subtasks`, `task_label_links` and `project_milestones`, exactly where
-- the board already is. Instantiating copies those rows into a new project
-- (docs/design/projects.md, "Milestones and templates").
--
-- Rejected: a separate template schema (this table holding a JSON shape). A
-- template that is not itself a project cannot be opened, reviewed or
-- corrected in the UI that already exists, and it drifts from the model it
-- claims to copy the first time a task gains a field. A template that IS a
-- project means the template editor is the board editor — one file, one
-- reason to change, and one model to keep true.
--
-- Only a `team` board may be marked. The list of templates is tenant-wide, so
-- a personal board in it would hand a colleague's private work to everybody
-- who opens the dialog. The store refuses it with a named rule; the shape
-- here cannot express the difference, because a mark is only ever a mark.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE project_templates (
    tenant_id  TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    -- The board that is the template. Also the key: a project is reusable or
    -- it is not, so the question cannot be asked twice.
    project_id TEXT NOT NULL,
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, project_id),
    -- The board owns the mark: delete the project and the mark goes with it,
    -- because a template that is not a project is not a template.
    CONSTRAINT project_templates_project_fk FOREIGN KEY (tenant_id, project_id)
        REFERENCES task_projects (tenant_id, id) ON DELETE CASCADE
);

-- "The templates of this tenant, newest mark last" — the one read the
-- create-from-template dialog makes.
CREATE INDEX project_templates_by_tenant
    ON project_templates (tenant_id, created_at, project_id);
