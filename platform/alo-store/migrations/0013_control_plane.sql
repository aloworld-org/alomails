-- Multi-tenant control plane (ADR 0012): a platform-operator tier above
-- tenant admins, a tenant lifecycle status, and tenant->domain ownership —
-- the security spine of shared hosting. All additive (expand-only): every
-- column has a safe default and existing rows stay valid, so this is not a
-- destructive migration.

-- The platform operator: a user (in the reserved `_platform` system tenant)
-- who governs tenants across the deployment. Distinct from `is_admin`, which
-- is confined to a single tenant; this flag is the ONLY cross-tenant role,
-- and it grants control operations, never read access to a tenant's data.
ALTER TABLE users ADD COLUMN is_platform_admin BOOLEAN NOT NULL DEFAULT FALSE;

-- Tenant lifecycle. A suspended tenant fails auth closed and its inbound mail
-- is deferred (transient 450, so senders retry rather than bounce), reversibly
-- and without touching any data.
ALTER TABLE tenants
    ADD COLUMN status TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'suspended'));

-- Tenant -> domain ownership. One domain belongs to exactly one tenant (the
-- PRIMARY KEY makes a second claim impossible). `verified_at` is stamped once
-- the DNS TXT proof at `_ficina-verify.<domain>` matching `verify_token` is
-- observed. Assigning a mailbox/alias/list address requires a verified domain
-- (enforced when FICINA_ENFORCE_DOMAIN_OWNERSHIP is on).
CREATE TABLE domains (
    domain       TEXT PRIMARY KEY,
    tenant_id    TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    verify_token TEXT NOT NULL,
    verified_at  TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX domains_tenant ON domains (tenant_id);
