-- Tasks (the third leg of the mail + calendar + tasks wedge). One task is one
-- row; board (kanban) and list are two groupings of the same rows (ADR 0022),
-- so a view-switch is a re-render and a card move is a single-field update.
-- Personal and team are one model, differing only by the project's scope
-- (ADR 0021). Tenant-scoped by construction like the rest of alo.

-- A project groups tasks and expresses personal-vs-team. A `personal` project is
-- auto-created per user (id `proj_personal_<user>`), private to its owner; a
-- `team` project is created explicitly and shared (v1: tenant-wide).
CREATE TABLE task_projects (
    tenant_id     TEXT NOT NULL,
    id            TEXT NOT NULL,
    name          TEXT NOT NULL,
    -- 'personal' (private to owner) or 'team' (shared).
    kind          TEXT NOT NULL DEFAULT 'team',
    owner_user_id TEXT NOT NULL,
    color         TEXT,
    archived      BOOLEAN NOT NULL DEFAULT false,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id)
);

-- The core record. Everything a view needs is a column here.
CREATE TABLE tasks (
    tenant_id        TEXT NOT NULL,
    id               TEXT NOT NULL,
    project_id       TEXT NOT NULL,
    title            TEXT NOT NULL,
    description      TEXT,
    -- The board column; free text so custom columns are possible later.
    status           TEXT NOT NULL DEFAULT 'todo',
    -- Fractional order WITHIN the status column: a reorder averages neighbours
    -- and writes one row (ADR 0022).
    position         DOUBLE PRECISION NOT NULL DEFAULT 0,
    assignee_user_id TEXT,
    due_at           TIMESTAMPTZ,
    -- 'none' | 'low' | 'medium' | 'high'.
    priority         TEXT NOT NULL DEFAULT 'none',
    -- 'active' (real work) or 'proposed' (AI suggestion awaiting approval,
    -- ADR 0023 — never shown as active work until accepted).
    state            TEXT NOT NULL DEFAULT 'active',
    -- The source link: the email/event this task came from ('email' | 'event').
    source_kind      TEXT,
    source_id        TEXT,
    created_by       TEXT NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at     TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, id)
);

-- "Tasks on this project's board", the hot path — grouped by status, ordered.
CREATE INDEX tasks_by_project ON tasks (tenant_id, project_id, status, position);
-- "My plate" + the calendar's due-task overlay: by assignee and due date.
CREATE INDEX tasks_by_assignee_due ON tasks (tenant_id, assignee_user_id, due_at);

-- A lightweight checklist inside a task (title + done), ordered.
CREATE TABLE task_subtasks (
    tenant_id  TEXT NOT NULL,
    id         TEXT NOT NULL,
    task_id    TEXT NOT NULL,
    title      TEXT NOT NULL,
    done       BOOLEAN NOT NULL DEFAULT false,
    position   DOUBLE PRECISION NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id)
);
CREATE INDEX task_subtasks_by_task ON task_subtasks (tenant_id, task_id, position);

CREATE TABLE task_comments (
    tenant_id      TEXT NOT NULL,
    id             TEXT NOT NULL,
    task_id        TEXT NOT NULL,
    author_user_id TEXT NOT NULL,
    body           TEXT NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id)
);
CREATE INDEX task_comments_by_task ON task_comments (tenant_id, task_id, created_at);

-- The task's history (created / status_changed / assigned / due_changed /
-- commented / accepted …); `detail` carries the specifics as JSON.
CREATE TABLE task_activity (
    tenant_id      TEXT NOT NULL,
    id             TEXT NOT NULL,
    task_id        TEXT NOT NULL,
    actor_user_id  TEXT NOT NULL,
    kind           TEXT NOT NULL,
    detail         JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id)
);
CREATE INDEX task_activity_by_task ON task_activity (tenant_id, task_id, created_at);

-- Attachments reuse the tenant blob store (upload wiring is a follow-up).
CREATE TABLE task_attachments (
    tenant_id  TEXT NOT NULL,
    id         TEXT NOT NULL,
    task_id    TEXT NOT NULL,
    blob_id    TEXT NOT NULL,
    filename   TEXT NOT NULL,
    size       BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id)
);
CREATE INDEX task_attachments_by_task ON task_attachments (tenant_id, task_id, created_at);
