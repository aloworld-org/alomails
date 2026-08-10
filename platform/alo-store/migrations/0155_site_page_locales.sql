-- S2.01b: localized drafts share one stable site page identity. The base row
-- remains the editor's current projection while content_locale records which
-- language that projection actually contains; translations live beside it.

ALTER TABLE site_pages ADD COLUMN content_locale TEXT;

UPDATE site_pages p
SET content_locale = s.default_locale
FROM sites s
WHERE s.tenant_id = p.tenant_id AND s.id = p.site_id;

ALTER TABLE site_pages ALTER COLUMN content_locale SET NOT NULL;

CREATE TABLE site_page_locales (
    tenant_id       TEXT NOT NULL,
    site_id         TEXT NOT NULL,
    page_id         TEXT NOT NULL,
    locale          TEXT NOT NULL,
    slug            TEXT NOT NULL,
    title           TEXT NOT NULL,
    sections        JSONB NOT NULL,
    seo_title       TEXT,
    seo_description TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, page_id, locale),
    FOREIGN KEY (tenant_id, page_id) REFERENCES site_pages (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, site_id) REFERENCES sites (tenant_id, id) ON DELETE CASCADE
);

-- A URL spelling is unique within one site and language, but may be translated
-- independently in every other language.
CREATE UNIQUE INDEX site_page_locales_slug_unique
    ON site_page_locales (tenant_id, site_id, locale, slug);
