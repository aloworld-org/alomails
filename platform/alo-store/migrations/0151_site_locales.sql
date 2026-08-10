-- Sites S2 multilingual foundation. Locale tags are normalized and validated
-- by the store before they reach these columns. The database keeps the two
-- structural invariants as a final guard: at least one language is enabled,
-- and the default language is one of them.

ALTER TABLE sites
    ADD COLUMN default_locale TEXT NOT NULL DEFAULT 'en',
    ADD COLUMN enabled_locales TEXT[] NOT NULL DEFAULT ARRAY['en']::TEXT[],
    ADD CONSTRAINT sites_enabled_locales_count
        CHECK (cardinality(enabled_locales) BETWEEN 1 AND 12),
    ADD CONSTRAINT sites_default_locale_enabled
        CHECK (default_locale = ANY(enabled_locales));
