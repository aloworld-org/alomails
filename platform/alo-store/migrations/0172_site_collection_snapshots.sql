-- Immutable alo Sites collection rows frozen with one publish. The source
-- Base table remains editable, while public rendering reads only this JSON
-- snapshot. Collection ids deliberately have no draft FK: history must
-- survive disconnecting or deleting the editable binding.

CREATE TABLE site_collection_snapshots (
    tenant_id    TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    publish_id   TEXT NOT NULL,
    collection_id TEXT NOT NULL,
    name         TEXT NOT NULL,
    items        JSONB NOT NULL,
    PRIMARY KEY (tenant_id, publish_id, collection_id),
    FOREIGN KEY (tenant_id, publish_id)
        REFERENCES site_publishes(tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX site_collection_snapshots_by_publish
    ON site_collection_snapshots (tenant_id, publish_id, collection_id);
