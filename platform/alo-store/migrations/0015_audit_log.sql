-- Tenant-scoped audit log (ADR 0012 / features.md [L]). Every administrative
-- mutation records who did what to which target and when, so a tenant admin can
-- answer "who changed this". Platform-operator actions on a tenant are recorded
-- under that tenant too, with a NULL actor_user_id (the operator is not one of
-- the tenant's users) and an actor_label instead. Additive, no behavior change.
CREATE TABLE audit_log (
    id             TEXT PRIMARY KEY,
    tenant_id      TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    actor_user_id  TEXT,
    actor_label    TEXT,
    action         TEXT NOT NULL,
    target         TEXT,
    detail         TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Read pattern: newest-first within a tenant.
CREATE INDEX audit_log_tenant_time ON audit_log (tenant_id, created_at DESC);
