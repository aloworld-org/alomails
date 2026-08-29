-- Tenant-scoped price connections replace the browser-only fixture formerly
-- used by Billing. A connection is a commercial relationship and its status;
-- credentials and remote payloads deliberately do not live here.
CREATE TABLE billing_price_connections (
    tenant_id       TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id              TEXT NOT NULL,
    direction       TEXT NOT NULL,
    company         TEXT NOT NULL,
    catalogue       TEXT NOT NULL,
    health          TEXT NOT NULL DEFAULT 'connected',
    cadence         TEXT NOT NULL DEFAULT 'daily',
    channel         TEXT NOT NULL DEFAULT 'alo',
    changes_count   INTEGER NOT NULL DEFAULT 0,
    last_synced_at  TIMESTAMPTZ,
    expires_at      TIMESTAMPTZ,
    created_by      TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    CONSTRAINT billing_price_connections_direction_known
        CHECK (direction IN ('received', 'shared')),
    CONSTRAINT billing_price_connections_health_known
        CHECK (health IN ('connected', 'attention', 'paused', 'expired')),
    CONSTRAINT billing_price_connections_cadence_known
        CHECK (cadence IN ('hourly', 'daily', 'weekly', 'manual', 'live', 'approval')),
    CONSTRAINT billing_price_connections_channel_known
        CHECK (channel IN ('alo', 'api')),
    CONSTRAINT billing_price_connections_company_shape
        CHECK (length(btrim(company)) > 0 AND char_length(company) <= 200),
    CONSTRAINT billing_price_connections_catalogue_shape
        CHECK (length(btrim(catalogue)) > 0 AND char_length(catalogue) <= 200),
    CONSTRAINT billing_price_connections_changes_range
        CHECK (changes_count >= 0)
);

CREATE INDEX billing_price_connections_list
    ON billing_price_connections (tenant_id, direction, updated_at DESC, id);

CREATE TABLE billing_price_connection_products (
    tenant_id     TEXT NOT NULL,
    connection_id TEXT NOT NULL,
    product_id    TEXT NOT NULL,
    PRIMARY KEY (tenant_id, connection_id, product_id),
    FOREIGN KEY (tenant_id, connection_id)
        REFERENCES billing_price_connections (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, product_id)
        REFERENCES billing_products (tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX billing_price_connection_products_by_product
    ON billing_price_connection_products (tenant_id, product_id);
