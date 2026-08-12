-- Immutable alo Sites catalog copies frozen with one publish. The editable
-- catalog keeps changing — a dish sells out, a price rises — while the public
-- pages of a publish keep showing exactly what was true when it was published,
-- until the tenant publishes again. Hidden items never reach this table.
--
-- Catalog ids deliberately have no foreign key to `site_catalogs`: publish
-- history must survive deleting the catalog it was taken from.

CREATE TABLE site_catalog_snapshots (
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    publish_id TEXT NOT NULL,
    catalog_id TEXT NOT NULL,
    name       TEXT NOT NULL,
    currency   TEXT NOT NULL,
    categories JSONB NOT NULL,
    items      JSONB NOT NULL,
    PRIMARY KEY (tenant_id, publish_id, catalog_id),
    FOREIGN KEY (tenant_id, publish_id)
        REFERENCES site_publishes(tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX site_catalog_snapshots_by_publish
    ON site_catalog_snapshots (tenant_id, publish_id, catalog_id);
