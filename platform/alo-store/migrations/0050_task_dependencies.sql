-- Task dependencies: a directed "blocked by" edge between two tasks in the same
-- tenant. `task_id` is blocked by `depends_on_task_id`; the Timeline draws an
-- arrow from the blocker to the blocked task. A pair is unique. Both endpoints
-- are always the caller's own tenant (enforced in the store by gating each id
-- through task visibility), and the whole table cascades with the tenant (law #1).

CREATE TABLE task_dependencies (
    tenant_id          TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    task_id            TEXT NOT NULL,
    depends_on_task_id TEXT NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, task_id, depends_on_task_id)
);
-- Reverse lookups ("what does this task block?") and edge roll-ups per tenant.
CREATE INDEX task_dependencies_by_dep ON task_dependencies (tenant_id, depends_on_task_id);
