-- alo Sites (ADR 0036): the pages of a site. Tenant-scoped and cascading
-- through the site (tenants -> sites -> site_pages), so deleting a site or a
-- tenant purges its pages. The `sections` JSON is validated against the typed
-- schema in `site_model` on every store write (docs/design/sites.md).

CREATE TABLE site_pages (
    tenant_id       TEXT NOT NULL,
    site_id         TEXT NOT NULL,
    id              TEXT NOT NULL,
    -- URL path segment, validated in the store: [a-z0-9-]{1,80}, no leading
    -- or trailing hyphen, never a reserved public path (blog, f, ...). The
    -- empty slug is the home page's spelling — enforced by the CHECK below.
    slug            TEXT NOT NULL,
    title           TEXT NOT NULL,
    -- Typed sections envelope {"schema_version": 1, "sections": [...]},
    -- schema-validated on every write; opaque to SQL.
    sections        JSONB NOT NULL DEFAULT '{"schema_version": 1, "sections": []}'::jsonb,
    -- SEO overrides; NULL means "derive from title / site defaults".
    seo_title       TEXT,
    seo_description TEXT,
    -- Position in the site's navigation, ascending.
    nav_order       INTEGER NOT NULL,
    is_home         BOOLEAN NOT NULL DEFAULT false,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, site_id) REFERENCES sites (tenant_id, id) ON DELETE CASCADE,
    -- Only the home page may live at the empty slug (the site root).
    CONSTRAINT site_pages_slug_shape CHECK (slug <> '' OR is_home)
);

-- One slug per site (tenant-scoped; slugs are not a cross-tenant surface).
CREATE UNIQUE INDEX site_pages_slug_unique ON site_pages (tenant_id, site_id, slug);

-- At most one home page per site.
CREATE UNIQUE INDEX site_pages_one_home ON site_pages (tenant_id, site_id) WHERE is_home;
