-- Privacy-preserving public site traffic. The aggregate table contains only
-- the dimensions owners can act on; the companion set contains opaque,
-- day-scoped visitor tokens used to increment unique counts exactly once.
-- There are deliberately no IP-address, user-agent, full-referrer, or raw
-- request columns in either table.
CREATE TABLE site_analytics_daily (
    tenant_id TEXT NOT NULL,
    site_id TEXT NOT NULL,
    day DATE NOT NULL,
    path TEXT NOT NULL CHECK (length(path) BETWEEN 1 AND 2048),
    referrer_domain TEXT NOT NULL CHECK (length(referrer_domain) <= 253),
    hits BIGINT NOT NULL CHECK (hits >= 0),
    unique_visitors BIGINT NOT NULL CHECK (unique_visitors >= 0),
    PRIMARY KEY (tenant_id, site_id, day, path, referrer_domain),
    FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites (tenant_id, id) ON DELETE CASCADE
);

CREATE TABLE site_analytics_daily_visitors (
    tenant_id TEXT NOT NULL,
    site_id TEXT NOT NULL,
    day DATE NOT NULL,
    path TEXT NOT NULL CHECK (length(path) BETWEEN 1 AND 2048),
    referrer_domain TEXT NOT NULL CHECK (length(referrer_domain) <= 253),
    visitor_hash BYTEA NOT NULL CHECK (octet_length(visitor_hash) = 32),
    PRIMARY KEY (
        tenant_id,
        site_id,
        day,
        path,
        referrer_domain,
        visitor_hash
    ),
    FOREIGN KEY (tenant_id, site_id, day, path, referrer_domain)
        REFERENCES site_analytics_daily (
            tenant_id,
            site_id,
            day,
            path,
            referrer_domain
        ) ON DELETE CASCADE
);

CREATE INDEX site_analytics_daily_site_day_idx
    ON site_analytics_daily (tenant_id, site_id, day DESC);
