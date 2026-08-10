-- S2.01c: a publish freezes the site's language contract and every exact
-- localized page draft. Public serving can therefore never follow later
-- locale, slug, SEO, or content edits until the next explicit publish.

ALTER TABLE site_publishes
    ADD COLUMN default_locale TEXT,
    ADD COLUMN enabled_locales TEXT[];

UPDATE site_publishes p
SET default_locale = s.default_locale,
    enabled_locales = s.enabled_locales
FROM sites s
WHERE s.tenant_id = p.tenant_id AND s.id = p.site_id;

ALTER TABLE site_publishes
    ALTER COLUMN default_locale SET NOT NULL,
    ALTER COLUMN enabled_locales SET NOT NULL,
    ADD CONSTRAINT site_publishes_enabled_locales_count
        CHECK (cardinality(enabled_locales) BETWEEN 1 AND 12),
    ADD CONSTRAINT site_publishes_default_locale_enabled
        CHECK (default_locale = ANY(enabled_locales));

ALTER TABLE site_page_snapshots ADD COLUMN locale TEXT;

UPDATE site_page_snapshots sn
SET locale = p.default_locale
FROM site_publishes p
WHERE p.tenant_id = sn.tenant_id AND p.id = sn.publish_id;

ALTER TABLE site_page_snapshots
    ALTER COLUMN locale SET NOT NULL,
    DROP CONSTRAINT site_page_snapshots_pkey,
    ADD PRIMARY KEY (tenant_id, publish_id, page_id, locale);

CREATE UNIQUE INDEX site_page_snapshots_locale_slug_unique
    ON site_page_snapshots (tenant_id, publish_id, locale, slug);

CREATE UNIQUE INDEX site_page_snapshots_locale_home_unique
    ON site_page_snapshots (tenant_id, publish_id, locale)
    WHERE is_home;
