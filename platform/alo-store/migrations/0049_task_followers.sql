-- Task followers: which users are watching a task (they'd get notified of
-- changes once notifications land). A follower is a (task, user) pair. The
-- creator is added on create; users follow/unfollow explicitly. Cascades with
-- the tenant (law #1).

CREATE TABLE task_followers (
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    task_id    TEXT NOT NULL,
    user_id    TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, task_id, user_id)
);
CREATE INDEX task_followers_by_task ON task_followers (tenant_id, task_id);
