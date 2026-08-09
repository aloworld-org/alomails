-- alo Sites blog posts (migration 0138). The body remains an alo Docs document;
-- this table stores only the site-facing metadata. Composite foreign keys
-- keep every reference inside one tenant even if a caller bypasses the store.

CREATE TABLE site_posts (
    tenant_id     TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    site_id       TEXT NOT NULL,
    id            TEXT NOT NULL,
    doc_node_id   TEXT NOT NULL,
    slug          TEXT NOT NULL,
    title         TEXT NOT NULL,
    excerpt       TEXT NOT NULL DEFAULT '',
    cover_blob_id TEXT,
    status        TEXT NOT NULL DEFAULT 'draft',
    published_at  TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    CONSTRAINT site_posts_site_fk
        FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites(tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT site_posts_doc_fk
        FOREIGN KEY (tenant_id, doc_node_id)
        REFERENCES drive_nodes(tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT site_posts_status_valid
        CHECK (status IN ('draft', 'published')),
    CONSTRAINT site_posts_publish_shape
        CHECK ((status = 'draft' AND published_at IS NULL)
            OR (status = 'published' AND published_at IS NOT NULL))
);

CREATE UNIQUE INDEX site_posts_slug_unique
    ON site_posts (tenant_id, site_id, slug);
CREATE UNIQUE INDEX site_posts_doc_unique
    ON site_posts (tenant_id, site_id, doc_node_id);
CREATE INDEX site_posts_by_site
    ON site_posts (tenant_id, site_id, created_at DESC, id);
