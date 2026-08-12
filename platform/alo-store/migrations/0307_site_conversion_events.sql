-- Aggregate conversion events for published sites (ADR 0036, S2.10a): how
-- often a site's own conversion point was seen, started, and completed.
--
-- The privacy shape follows the rest of the Sites analytics family, with one
-- addition that is the whole point of this table:
--
--   * **The identity here belongs to the site, never to the visitor.** A row
--     is keyed by a source the tenant itself created — today a contact form,
--     whose id is already public in the page's own markup — so attribution
--     needs no tracking id, no cookie and no visitor token. There is no
--     column one could be put in.
--   * **Three independent counters, never a journey.** view, start and submit
--     are counted separately; nothing records that one browser did two of
--     them, so a funnel read from this table is a ratio of totals and can
--     never be resolved to a person.
--   * **No time of day.** The day is the finest grain, exactly as for page
--     views and heatmap cells.
--
-- Cardinality is bounded by construction: the source id must resolve to a row
-- the tenant owns (site_forms, capped at 50 per site), so a visitor's browser
-- cannot open new buckets by inventing ids the way it can with a page path.
--
-- source_kind is a word rather than a boolean because the later commerce and
-- booking slices convert on their own site-owned objects; adding one is an
-- additive check-constraint change, and the rows written today keep meaning
-- exactly what they mean now.
CREATE TABLE site_conversion_daily (
    tenant_id   TEXT NOT NULL,
    site_id     TEXT NOT NULL,
    day         DATE NOT NULL,
    source_kind TEXT NOT NULL CHECK (source_kind IN ('form')),
    -- The site-owned id of the conversion point (a site_forms id today).
    -- Deliberately not a foreign key: the counts are an immutable record of
    -- what happened, and deleting the form that produced them must not
    -- silently rewrite last month's report.
    source_id   TEXT NOT NULL CHECK (length(source_id) BETWEEN 1 AND 64),
    stage       TEXT NOT NULL CHECK (stage IN ('view', 'start', 'submit')),
    hits        BIGINT NOT NULL CHECK (hits >= 0),
    PRIMARY KEY (tenant_id, site_id, day, source_kind, source_id, stage),
    FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites (tenant_id, id) ON DELETE CASCADE
);

-- The owner's report reads one site over a period.
CREATE INDEX site_conversion_daily_site_day_idx
    ON site_conversion_daily (tenant_id, site_id, day DESC);
