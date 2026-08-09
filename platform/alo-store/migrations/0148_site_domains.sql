-- Custom public hosts for alo Sites. The domain is a single deployment-wide
-- namespace, while every read and mutation is anchored to the owning
-- tenant/site pair. Verification proves DNS control before serving can mark a
-- domain live (S1.25b).
CREATE TABLE site_domains (
    tenant_id TEXT NOT NULL,
    site_id TEXT NOT NULL,
    domain TEXT NOT NULL CHECK (
        domain = lower(domain)
        AND length(domain) BETWEEN 4 AND 253
    ),
    verify_token TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'verified', 'live')),
    verified_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, site_id, domain),
    FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites (tenant_id, id) ON DELETE CASCADE,
    CHECK (
        (status = 'pending' AND verified_at IS NULL)
        OR (status IN ('verified', 'live') AND verified_at IS NOT NULL)
    )
);

-- A Host can route to exactly one site. Conflicts reveal only that the name is
-- already connected, never the owning tenant or site.
CREATE UNIQUE INDEX site_domains_domain_unique ON site_domains (domain);

CREATE INDEX site_domains_site_idx
    ON site_domains (tenant_id, site_id, created_at);
