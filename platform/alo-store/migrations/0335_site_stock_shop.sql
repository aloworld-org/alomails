-- The web shop's stock checkout — what a site sells from the shelf, and the
-- record of a buyer paying for it (migration 0335, ADR 0041, item S3.05a2).
--
-- Wave two of alo Commerce sells goods the way wave one sold seats, and the
-- same discipline holds everywhere: nothing here stores a second copy of a
-- price or a stock count. A shop item is a *reference* into Billing's price
-- list; what it costs is asked of the catalog seam at render and at sale, and
-- what is available is asked of Inventory's stock-sale seam (0334) at every
-- read. What IS new data, and therefore lives here, is the site's own facts:
-- which products its shop lists, what the site charges for delivery, and the
-- record of each sale.
--
-- The shipping rate is deliberately the site's own price — integer cents,
-- one flat rate per order — because delivery is something the *site* sells,
-- not a fact any other module owns. Its VAT follows the goods (an ancillary
-- cost takes the main supply's treatment), which is why no rate column
-- exists here: the order snapshots the goods' rate once, for both.
--
-- The order mirrors the ticket order (0330): the price fields are the record
-- of the sale as it was struck, never a copy of the price list; payments are
-- never ours (the provider's opaque id and checkout URL are all that is
-- stored, and the privacy proof in tests/site_public_stock.rs asserts the
-- column list, so a column that could carry card data cannot appear without
-- failing a test). One hold is one order (site_stock_orders_one_per_hold);
-- one provider payment settles one order (site_stock_orders_one_payment).
-- Unlike a ticket, a stock sale ships somewhere, so the order carries the
-- buyer's delivery address — the tenant's own record of their own sale,
-- exactly like a form submission.

CREATE TABLE site_shop_items (
    id         TEXT PRIMARY KEY,
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    site_id    TEXT NOT NULL,
    -- The reference into Billing's price list. Never a copied price or name:
    -- the catalog seam (billing_catalog_read) answers what this is called
    -- and what it costs *now*, and Inventory's stock-sale seam answers what
    -- is on the shelf.
    product_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT site_shop_items_site_fk
        FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites(tenant_id, id) ON DELETE CASCADE,
    -- No ON DELETE action on purpose (0157's choice): a product listed on a
    -- live shop cannot be deleted out from under it; archiving it simply
    -- stops the catalog seam answering, and the listing goes dark.
    CONSTRAINT site_shop_items_product_fk
        FOREIGN KEY (tenant_id, product_id)
        REFERENCES billing_products (tenant_id, id),
    CONSTRAINT site_shop_items_one_listing UNIQUE (tenant_id, site_id, product_id),
    CONSTRAINT site_shop_items_tenant_scoped UNIQUE (tenant_id, id)
);

CREATE INDEX site_shop_items_by_site
    ON site_shop_items (tenant_id, site_id, created_at, id);

-- One row per site that has said anything about its shop; a site with no row
-- ships for 0 cents. Expand-only home for later shop-wide settings.
CREATE TABLE site_shop_settings (
    tenant_id      TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    site_id        TEXT NOT NULL,
    -- The site's own flat delivery price per order, integer cents in the
    -- tenant's accounting currency. Not a copy of anything.
    shipping_cents BIGINT NOT NULL DEFAULT 0 CHECK (shipping_cents >= 0),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, site_id),
    CONSTRAINT site_shop_settings_site_fk
        FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites(tenant_id, id) ON DELETE CASCADE
);

CREATE TABLE site_stock_orders (
    id                  TEXT PRIMARY KEY,
    tenant_id           TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    site_id             TEXT NOT NULL,
    product_id          TEXT NOT NULL,
    -- The Inventory hold (0334) whose goods this order buys — also the
    -- creation's replay token: one hold is one order, ever.
    hold_id             TEXT NOT NULL,
    units               BIGINT NOT NULL CHECK (units >= 1),
    buyer_name          TEXT NOT NULL,
    buyer_email         TEXT NOT NULL,
    -- Where the goods go: the buyer's own statement, recorded as the
    -- tenant's record of their sale. Country is ISO 3166-1 alpha-2.
    ship_to_line        TEXT NOT NULL,
    ship_to_city        TEXT NOT NULL,
    ship_to_postcode    TEXT NOT NULL,
    ship_to_country     TEXT NOT NULL,
    -- The sale as it was struck: integer cents, VAT in basis points, the
    -- tenant's accounting currency. amount_cents = units * unit_price_cents
    -- + shipping_cents, computed server-side when the order was placed; the
    -- VAT rate is the goods' rate, and shipping follows it.
    unit_price_cents    BIGINT NOT NULL CHECK (unit_price_cents >= 0),
    shipping_cents      BIGINT NOT NULL CHECK (shipping_cents >= 0),
    amount_cents        BIGINT NOT NULL CHECK (amount_cents >= 0),
    vat_rate_bp         INTEGER NOT NULL CHECK (vat_rate_bp >= 0),
    currency            TEXT NOT NULL,
    -- pending: placed, provider not asked yet. awaiting_payment: the
    -- provider minted a payment and the buyer is on its page. paid: money
    -- confirmed, the goods claimed off the shelf in the same act. failed /
    -- cancelled / expired are terminal; 'failure' carries the sentence the
    -- tenant can act on when one needs acting on (a payment whose goods
    -- could not be claimed names the refund).
    state               TEXT NOT NULL DEFAULT 'pending'
                        CHECK (state IN ('pending', 'awaiting_payment', 'paid',
                                         'failed', 'cancelled', 'expired')),
    provider_payment_id TEXT,
    checkout_url        TEXT,
    failure             TEXT,
    paid_at             TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL,
    updated_at          TIMESTAMPTZ NOT NULL,
    CONSTRAINT site_stock_orders_site_fk
        FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites(tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT site_stock_orders_product_fk
        FOREIGN KEY (tenant_id, product_id)
        REFERENCES billing_products (tenant_id, id),
    CONSTRAINT site_stock_orders_hold_fk
        FOREIGN KEY (tenant_id, hold_id)
        REFERENCES inv_stock_sale_holds (tenant_id, id),
    CONSTRAINT site_stock_orders_one_per_hold UNIQUE (tenant_id, hold_id),
    CONSTRAINT site_stock_orders_tenant_scoped UNIQUE (tenant_id, id)
);

-- The webhook's door: a provider payment id names exactly one order, across
-- all tenants — the row itself then names the tenant it belongs to.
CREATE UNIQUE INDEX site_stock_orders_one_payment
    ON site_stock_orders (provider_payment_id)
    WHERE provider_payment_id IS NOT NULL;

CREATE INDEX site_stock_orders_by_site
    ON site_stock_orders (tenant_id, site_id, created_at DESC, id);

-- What one paid stock order produced: the invoice Billing raised and the CRM
-- outcome, each written by the background sweep through the owning module's
-- own door. One paid order is fulfilled exactly once
-- (site_stock_fulfilments_one_per_order): the sweep claims by inserting this
-- row, so two concurrent sweeps cannot fulfil the same sale twice. No buyer
-- column of any kind — who bought lives on the order. No token either: a
-- stock sale has no ticket page; the buyer's proof is the return page and
-- the invoice.
CREATE TABLE site_stock_fulfilments (
    id             TEXT PRIMARY KEY,
    tenant_id      TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    site_id        TEXT NOT NULL,
    order_id       TEXT NOT NULL,
    -- What was sold, as the invoice line said it — written once at
    -- fulfilment, the record of the sale, never a copy of the price list.
    description    TEXT NOT NULL DEFAULT '',
    invoice_id     TEXT,
    invoice_number TEXT,
    invoice_note   TEXT,
    crm_outcome    TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT site_stock_fulfilments_site_fk
        FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites(tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT site_stock_fulfilments_order_fk
        FOREIGN KEY (tenant_id, order_id)
        REFERENCES site_stock_orders(tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT site_stock_fulfilments_one_per_order UNIQUE (tenant_id, order_id),
    CONSTRAINT site_stock_fulfilments_tenant_scoped UNIQUE (tenant_id, id)
);

CREATE INDEX site_stock_fulfilments_by_site
    ON site_stock_fulfilments (tenant_id, site_id, created_at DESC, id);
