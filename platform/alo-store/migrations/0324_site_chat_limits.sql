-- Cost and abuse control for the site visitor assistant (migration 0324,
-- ADR 0040 §3, item S3.02c). An anonymous endpoint that calls a metered model
-- is a bill any stranger can run up, so every site carries a monthly spending
-- ceiling that is DEFAULTED rather than blank, and the assistant itself is off
-- until the tenant switches it on.
--
-- `site_chat_settings` is the tenant's choice: whether the assistant answers
-- at all, and how much it may spend per calendar month. Absence of a row means
-- "off, default ceiling" — the fail-closed reading.
--
-- `site_chat_spend` is the ledger: one row per site per UTC month ('YYYY-MM'),
-- integer cents only (the money law). `ceiling_hit_at` is stamped exactly once,
-- by the spend write that crosses the ceiling; `hit_notified_at` is the
-- at-most-once claim of the owner-notification sweep, the same posture as the
-- form/order/booking notifications.

CREATE TABLE site_chat_settings (
    tenant_id             TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    site_id               TEXT NOT NULL,
    enabled               BOOLEAN NOT NULL DEFAULT FALSE,
    -- Spend, not tokens: integer euro cents per calendar month.
    monthly_ceiling_cents BIGINT NOT NULL DEFAULT 1000,
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, site_id),
    CONSTRAINT site_chat_settings_site_fk
        FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites(tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT site_chat_settings_ceiling_positive
        CHECK (monthly_ceiling_cents > 0)
);

CREATE TABLE site_chat_spend (
    tenant_id       TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    site_id         TEXT NOT NULL,
    -- UTC calendar month, 'YYYY-MM'. A new month starts a fresh budget.
    month           TEXT NOT NULL,
    spent_cents     BIGINT NOT NULL DEFAULT 0,
    ceiling_hit_at  TIMESTAMPTZ,
    hit_notified_at TIMESTAMPTZ,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, site_id, month),
    CONSTRAINT site_chat_spend_site_fk
        FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites(tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT site_chat_spend_never_negative
        CHECK (spent_cents >= 0)
);

-- The notification sweep's question — "which hit ceilings has nobody been
-- told about?" — is almost always answered "none"; keep it a read of the few
-- pending rows rather than a scan of every ledger month ever written.
CREATE INDEX site_chat_spend_pending_notification
    ON site_chat_spend (ceiling_hit_at, site_id)
    WHERE ceiling_hit_at IS NOT NULL AND hit_notified_at IS NULL;
