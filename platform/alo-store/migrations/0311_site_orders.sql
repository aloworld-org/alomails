-- Order forms on alo Sites catalogs (ADR 0036, ADR 0041 wave zero): a visitor
-- asks for things from a published catalog, and the request lands in the
-- owner's inbox and order list. There is deliberately NO checkout here — no
-- payment, no reservation, no stock. An order is a request the owner reads,
-- confirms and fulfils by hand, which is what a bakery taking Saturday orders
-- actually needs; paid checkout is wave three (ADR 0041) and will be built on
-- top of this record rather than beside it.
--
-- `orders_enabled` lives on the catalog AND on the frozen snapshot: what a
-- published page offers must be what the public door accepts, so the door
-- reads the published copy and a toggle takes effect at the next publish,
-- exactly like a price change.

ALTER TABLE site_catalogs
    ADD COLUMN orders_enabled BOOLEAN NOT NULL DEFAULT false;

ALTER TABLE site_catalog_snapshots
    ADD COLUMN orders_enabled BOOLEAN NOT NULL DEFAULT false;

-- One order request from a visitor. The catalog is named by id and by the
-- name it carried when the order arrived, with no foreign key: an order is a
-- record of what happened and must survive deleting the catalog it came from.
--
-- Privacy: the columns are what the visitor typed plus what the owner needs to
-- answer them. Nothing about the connection is stored — no IP, no user agent —
-- exactly like a form submission (`docs/design/sites.md`, privacy model).
CREATE TABLE site_orders (
    tenant_id      TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    site_id        TEXT NOT NULL,
    id             TEXT NOT NULL,
    catalog_id     TEXT NOT NULL,
    catalog_name   TEXT NOT NULL,
    currency       TEXT NOT NULL,
    customer_name  TEXT NOT NULL,
    customer_email TEXT NOT NULL,
    customer_phone TEXT,
    note           TEXT,
    -- Sum of the priced lines in minor units of `currency`. Lines whose item
    -- carried no price (an enquiry-only service) contribute nothing and are
    -- visible as such on the line itself — the owner quotes them by hand.
    total_cents    BIGINT NOT NULL,
    status         TEXT NOT NULL DEFAULT 'new',
    received_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- NULL until the owner-notification sweep has claimed this order.
    notified_at    TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites(tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT site_orders_status
        CHECK (status IN ('new', 'confirmed', 'fulfilled', 'cancelled')),
    CONSTRAINT site_orders_total_non_negative CHECK (total_cents >= 0),
    CONSTRAINT site_orders_currency_iso CHECK (currency ~ '^[A-Z]{3}$')
);

CREATE INDEX site_orders_by_site
    ON site_orders (tenant_id, site_id, received_at DESC, id DESC);

CREATE INDEX site_orders_awaiting_notification
    ON site_orders (received_at, id) WHERE notified_at IS NULL;

-- One requested item, frozen with the order: the name and unit price the
-- visitor saw on the published page, so the record still reads correctly after
-- the catalog moves on. `item_slug` is the published handle, kept for the
-- owner's own lookup; it is not a foreign key for the same reason.
CREATE TABLE site_order_lines (
    tenant_id        TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    order_id         TEXT NOT NULL,
    position         INTEGER NOT NULL,
    item_slug        TEXT NOT NULL,
    item_name        TEXT NOT NULL,
    quantity         INTEGER NOT NULL,
    unit_price_cents BIGINT,
    line_total_cents BIGINT,
    PRIMARY KEY (tenant_id, order_id, position),
    FOREIGN KEY (tenant_id, order_id)
        REFERENCES site_orders(tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT site_order_lines_quantity_positive CHECK (quantity >= 1),
    CONSTRAINT site_order_lines_price_non_negative
        CHECK (unit_price_cents IS NULL OR unit_price_cents >= 0),
    CONSTRAINT site_order_lines_total_agrees
        CHECK ((unit_price_cents IS NULL) = (line_total_cents IS NULL))
);
