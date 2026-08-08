-- alo Projects (ADR 0035, wave B3.09): milestones — the named dates a plan is
-- made of — and the link that puts one task under one of them.
--
-- A milestone is a NAMED DATE ON A PROJECT, nothing more: "Design signed off,
-- 30 September". The timeline the UI draws is a rendering of these rows over
-- the board that already exists (docs/design/projects.md, "Milestones and
-- templates"), not a second model of the work — the tasks are the same tasks,
-- read through `task_milestones`.
--
-- Two decisions are in the shape of these tables.
--
-- `task_milestones` is a SIDE TABLE keyed on the task, exactly as
-- `project_clients` is keyed on the project (0122): `tasks.rs` gains no column
-- and no reason to change (law 3), and the primary key on `task_id` means "which
-- milestone is this task in" has exactly one answer. A task under two milestones
-- is a plan that cannot be drawn.
--
-- A milestone is DONE WHEN A HUMAN SAYS SO (`done_at`), never when its tasks
-- are. A plan whose dates and states move themselves is a plan nobody trusts,
-- and "the last task closed" is not the same statement as "the client accepted
-- the deliverable".
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE project_milestones (
    tenant_id  TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id         TEXT NOT NULL,
    -- The board this date belongs to. Any project the caller can see may carry
    -- milestones — unlike client facts, a plan is not a claim about money, and
    -- a personal board with three dates on it is an ordinary use of one.
    project_id TEXT NOT NULL,
    name       TEXT NOT NULL,
    -- The day itself. NOT NULL: a milestone without a date is a label, and the
    -- timeline has nowhere to draw it. A DATE, not a timestamp — a deadline
    -- falls on a day in the tenant's world, not at an instant in UTC.
    due_on     DATE NOT NULL,
    -- When a human marked it reached, or NULL while it is still ahead.
    done_at    TIMESTAMPTZ,
    -- Tie-break within one day, assigned on create. The read order is
    -- (due_on, position), so two milestones on the same date keep the order
    -- they were planned in rather than an accidental one.
    position   BIGINT NOT NULL DEFAULT 0,
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- The board owns the plan: delete the project and its milestones go with
    -- it, because dates on a project that no longer exists are not dates.
    CONSTRAINT project_milestones_project_fk FOREIGN KEY (tenant_id, project_id)
        REFERENCES task_projects (tenant_id, id) ON DELETE CASCADE,
    -- Defence in depth: the store validates the name before writing, so a
    -- violation here means a bug in our code, not bad user input.
    CONSTRAINT project_milestones_name_present CHECK (btrim(name) <> '')
);

-- "The plan of this project", in the order it is drawn. Every read of this
-- table is by project and ordered by date.
CREATE INDEX project_milestones_by_project
    ON project_milestones (tenant_id, project_id, due_on, position);

CREATE TABLE task_milestones (
    tenant_id    TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    -- The key: one milestone per task, so the timeline can place every task
    -- exactly once and "which milestone is this in" cannot be asked twice.
    task_id      TEXT NOT NULL,
    milestone_id TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, task_id),
    CONSTRAINT task_milestones_task_fk FOREIGN KEY (tenant_id, task_id)
        REFERENCES tasks (tenant_id, id) ON DELETE CASCADE,
    -- Deleting a milestone unplaces its tasks; it never deletes work. The
    -- tasks stay on the board exactly where they were.
    CONSTRAINT task_milestones_milestone_fk FOREIGN KEY (tenant_id, milestone_id)
        REFERENCES project_milestones (tenant_id, id) ON DELETE CASCADE
);

-- "The tasks under this milestone" — the count on a timeline bar and the list
-- inside it.
CREATE INDEX task_milestones_by_milestone
    ON task_milestones (tenant_id, milestone_id);
