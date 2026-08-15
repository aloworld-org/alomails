-- The site assistant's Public knowledge collection (migration 0323, ADR 0040
-- §1). One row = one tenant document deliberately published to the site's
-- visitor-facing assistant — whatever the assistant can read, the internet can
-- read, so every row here is the result of an explicit act. Composite foreign
-- keys keep every reference inside one tenant. The document reference cascades:
-- deleting the file from Drive silently removes it from the assistant's
-- knowledge (fail-closed), it never blocks the delete.

CREATE TABLE site_knowledge_sources (
    tenant_id   TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    site_id     TEXT NOT NULL,
    id          TEXT NOT NULL,
    doc_node_id TEXT NOT NULL,
    added_by    TEXT NOT NULL,
    added_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    CONSTRAINT site_knowledge_sources_site_fk
        FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites(tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT site_knowledge_sources_doc_fk
        FOREIGN KEY (tenant_id, doc_node_id)
        REFERENCES drive_nodes(tenant_id, id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX site_knowledge_sources_doc_unique
    ON site_knowledge_sources (tenant_id, site_id, doc_node_id);
CREATE INDEX site_knowledge_sources_by_site
    ON site_knowledge_sources (tenant_id, site_id, added_at, id);
