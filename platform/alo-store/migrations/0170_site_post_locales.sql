-- S2.01e: localized site-facing post metadata. Post bodies remain alo Docs
-- documents; every locale keeps the same stable post identity and document.

ALTER TABLE site_posts ADD COLUMN content_locale TEXT;

UPDATE site_posts p
SET content_locale = s.default_locale
FROM sites s
WHERE s.tenant_id = p.tenant_id AND s.id = p.site_id;

ALTER TABLE site_posts ALTER COLUMN content_locale SET NOT NULL;

CREATE TABLE site_post_locales (
    tenant_id  TEXT NOT NULL,
    site_id    TEXT NOT NULL,
    post_id    TEXT NOT NULL,
    locale     TEXT NOT NULL,
    slug       TEXT NOT NULL,
    title      TEXT NOT NULL,
    excerpt    TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, post_id, locale),
    FOREIGN KEY (tenant_id, post_id) REFERENCES site_posts (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, site_id) REFERENCES sites (tenant_id, id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX site_post_locales_slug_unique
    ON site_post_locales (tenant_id, site_id, locale, slug);
