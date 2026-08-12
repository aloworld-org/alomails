-- Second-generation traffic dimensions for public sites: which campaign
-- brought a visit, which country it came from (as the two-letter code a proxy
-- reports, never derived here from an address), which class of device it was
-- read on, and which page a visitor-day started and ended on.
--
-- The privacy model of 0142 is unchanged and deliberately narrower than the
-- data the request carried: the campaign is one bounded label taken from
-- `utm_campaign` while the rest of the query string is dropped, the device is
-- one of four words rather than a user agent, and no address, user agent,
-- full referrer, or raw request text is representable in either table.
CREATE TABLE site_analytics_dimension_daily (
    tenant_id TEXT NOT NULL,
    site_id TEXT NOT NULL,
    day DATE NOT NULL,
    dimension TEXT NOT NULL CHECK (
        dimension IN ('campaign', 'country', 'device', 'entry', 'exit')
    ),
    value TEXT NOT NULL CHECK (length(value) <= 2048),
    hits BIGINT NOT NULL CHECK (hits >= 0),
    PRIMARY KEY (tenant_id, site_id, day, dimension, value),
    FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites (tenant_id, id) ON DELETE CASCADE
);

-- The cursor that makes entry and exit pages computable without storing a
-- visitor journey: one row per site per day per opaque daily token, holding
-- only the page that token last looked at. It reveals nothing the per-day
-- visitor set of 0142 does not already hold, and it dies with the day's
-- token.
CREATE TABLE site_analytics_visitor_day (
    tenant_id TEXT NOT NULL,
    site_id TEXT NOT NULL,
    day DATE NOT NULL,
    visitor_hash BYTEA NOT NULL CHECK (octet_length(visitor_hash) = 32),
    last_path TEXT NOT NULL CHECK (length(last_path) BETWEEN 1 AND 2048),
    PRIMARY KEY (tenant_id, site_id, day, visitor_hash),
    FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites (tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX site_analytics_dimension_daily_site_day_idx
    ON site_analytics_dimension_daily (tenant_id, site_id, day DESC);
