-- Task labels (tags): reusable, tenant-scoped labels a task can carry (e.g.
-- "Design", "Website"). A label is defined once per tenant and linked to any
-- number of tasks; a task carries any number of labels. Both tables cascade
-- with the tenant (law #1: nothing survives a tenant delete).

CREATE TABLE task_labels (
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    id         TEXT NOT NULL,
    name       TEXT NOT NULL,
    -- Display colour (hex like `#4b83c4`), or null for the default accent.
    color      TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id)
);

-- The many-to-many link between a task and a label. A row is the fact "this
-- task has this label"; deleting a task or a label removes its links (handled
-- in the store's delete paths, which run in a transaction).
CREATE TABLE task_label_links (
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    task_id   TEXT NOT NULL,
    label_id  TEXT NOT NULL,
    PRIMARY KEY (tenant_id, task_id, label_id)
);
CREATE INDEX task_label_links_by_task ON task_label_links (tenant_id, task_id);
CREATE INDEX task_label_links_by_label ON task_label_links (tenant_id, label_id);
