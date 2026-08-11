-- alo Sites reusable collections backed by alo Base. A binding belongs to one
-- tenant/site and one table in that tenant. The stable field ids in `mapping`
-- are validated by the store before every write; display names are deliberately
-- not persisted because Base users may rename columns at any time.

CREATE TABLE site_collections (
    tenant_id     TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    site_id       TEXT NOT NULL,
    id            TEXT NOT NULL,
    name          TEXT NOT NULL,
    base_table_id TEXT NOT NULL,
    mapping       JSONB NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites(tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, base_table_id)
        REFERENCES base_tables(tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX site_collections_by_site
    ON site_collections (tenant_id, site_id, created_at, id);
