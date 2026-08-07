-- alo Sites (ADR 0036): the publish flow. A publish is an immutable record of
-- "what the internet sees": one `site_publishes` row (freezing the theme) plus
-- one `site_page_snapshots` row per page (freezing slug/title/sections/SEO/
-- nav). The site's `published_publish_id` pointer names the current set and is
-- flipped atomically by the store's publish transaction; the public service
-- reads ONLY snapshots, so drafts are unreachable by construction, not by
-- filtering (docs/design/sites.md).

CREATE TABLE site_publishes (
    tenant_id    TEXT NOT NULL,
    site_id      TEXT NOT NULL,
    id           TEXT NOT NULL,
    -- The site's theme envelope frozen at publish time.
    theme        JSONB NOT NULL,
    published_by TEXT NOT NULL,
    published_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, site_id) REFERENCES sites (tenant_id, id) ON DELETE CASCADE
);

-- Publish history of a site, newest first (the S2 rollback substrate).
CREATE INDEX site_publishes_by_site ON site_publishes (tenant_id, site_id, published_at DESC);

CREATE TABLE site_page_snapshots (
    tenant_id       TEXT NOT NULL,
    publish_id      TEXT NOT NULL,
    -- The draft page this froze. Deliberately NOT a foreign key: a snapshot
    -- must survive the page being edited or deleted — that is the whole point.
    page_id         TEXT NOT NULL,
    slug            TEXT NOT NULL,
    title           TEXT NOT NULL,
    sections        JSONB NOT NULL,
    seo_title       TEXT,
    seo_description TEXT,
    nav_order       INTEGER NOT NULL,
    is_home         BOOLEAN NOT NULL,
    PRIMARY KEY (tenant_id, publish_id, page_id),
    FOREIGN KEY (tenant_id, publish_id)
        REFERENCES site_publishes (tenant_id, id) ON DELETE CASCADE
);

-- The published-set pointer: NULL means nothing is live. Composite FK so the
-- pointer can only ever name a publish of the same tenant; no referential
-- action — publishes are only ever deleted by the site cascade, which removes
-- the pointing row in the same statement.
ALTER TABLE sites ADD COLUMN published_publish_id TEXT;
ALTER TABLE sites ADD CONSTRAINT sites_published_publish_fk
    FOREIGN KEY (tenant_id, published_publish_id)
    REFERENCES site_publishes (tenant_id, id);
