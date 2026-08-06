-- alo Sites (ADR 0036): a tenant's websites. Tenant-scoped like every other
-- table and cascading with the tenant (Law 1), with ONE deliberate global
-- surface: the subdomain unique index below spans all tenants, because
-- `<subdomain>.<SITES_DOMAIN>` is a single public namespace. The claim check
-- reveals only taken/free — never the owning tenant (docs/design/sites.md).

CREATE TABLE sites (
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    id         TEXT NOT NULL,
    name       TEXT NOT NULL,
    -- DNS-safe label, validated in the store: [a-z0-9-]{3,40}, no leading or
    -- trailing hyphen, and never a reserved word (www, mail, admin, ...).
    subdomain  TEXT NOT NULL,
    -- 'draft' | 'live' — flipped by the publish flow, never set directly.
    status     TEXT NOT NULL DEFAULT 'draft',
    -- Theme tokens (palette/typography preset + logo/favicon blob refs).
    -- Typed validation lands with the theme model; empty object until themed.
    theme      JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id)
);

-- The single cross-tenant surface: one public namespace of subdomains.
CREATE UNIQUE INDEX sites_subdomain_unique ON sites (subdomain);
