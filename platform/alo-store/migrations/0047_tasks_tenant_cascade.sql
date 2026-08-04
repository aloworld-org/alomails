-- Tasks tables must purge with their tenant (law #1: the tenant is sacred —
-- deleting a tenant leaves nothing behind). Migration 0046 created the six task
-- tables without a foreign key to `tenants`, so `DELETE FROM tenants` (the whole
-- tenant-deletion path, control.rs::delete_tenant) cascaded users and mail but
-- stranded task rows. This adds the same `tenant_id -> tenants(id) ON DELETE
-- CASCADE` every other tenant-scoped table already carries.

-- One-time cleanup: drop any rows already orphaned by a past tenant deletion,
-- so the constraint can be added without validation failures.
DELETE FROM task_attachments WHERE tenant_id NOT IN (SELECT id FROM tenants);
DELETE FROM task_activity    WHERE tenant_id NOT IN (SELECT id FROM tenants);
DELETE FROM task_comments    WHERE tenant_id NOT IN (SELECT id FROM tenants);
DELETE FROM task_subtasks    WHERE tenant_id NOT IN (SELECT id FROM tenants);
DELETE FROM tasks            WHERE tenant_id NOT IN (SELECT id FROM tenants);
DELETE FROM task_projects    WHERE tenant_id NOT IN (SELECT id FROM tenants);

-- Each task table references the tenant directly, so a single `DELETE FROM
-- tenants` cascades to all six (single-task deletes already clean their own
-- children transactionally in delete_task).
ALTER TABLE task_projects
    ADD CONSTRAINT task_projects_tenant_fk
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE;
ALTER TABLE tasks
    ADD CONSTRAINT tasks_tenant_fk
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE;
ALTER TABLE task_subtasks
    ADD CONSTRAINT task_subtasks_tenant_fk
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE;
ALTER TABLE task_comments
    ADD CONSTRAINT task_comments_tenant_fk
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE;
ALTER TABLE task_activity
    ADD CONSTRAINT task_activity_tenant_fk
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE;
ALTER TABLE task_attachments
    ADD CONSTRAINT task_attachments_tenant_fk
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE;
