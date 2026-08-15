-- The ticket order — the record of one buyer paying for one hold's seats
-- (migration 0330, ADR 0041, item S3.04c).
--
-- The hold (0329) is pure seat accounting and carries no buyer; the order is
-- where the buyer lives — who is paying, what they were shown, and where the
-- payment stands. The price fields are NOT a copy of the price list: they are
-- the record of a sale, the amount this buyer was actually charged at the
-- moment the order was placed, the same way a domain purchase records the
-- quote its buyer approved. The price list stays Billing's; a later price
-- change never rewrites what somebody already paid.
--
-- Payments are never ours (ADR 0041): the buyer pays on the provider's hosted
-- page, and the only thing stored here is the provider's opaque payment id
-- and where its checkout lives. There is deliberately NO column for a card
-- number, an expiry, a CVC or a cardholder — the privacy proof in
-- tests/site_ticket_orders.rs asserts the column list, so a column that could
-- carry card data cannot appear without failing a test.
--
-- One hold is one order (site_ticket_orders_one_per_hold): the hold id is the
-- caller's replay token, so a double-clicked buy button reaches the order it
-- already made. One provider payment settles one order
-- (site_ticket_orders_one_payment): a webhook replayed or forged against a
-- second order finds a unique index, not a second sale.

CREATE TABLE site_ticket_orders (
    id                  TEXT PRIMARY KEY,
    tenant_id           TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    site_id             TEXT NOT NULL,
    event_id            TEXT NOT NULL,
    hold_id             TEXT NOT NULL,
    quantity            INTEGER NOT NULL CHECK (quantity >= 1),
    buyer_name          TEXT NOT NULL,
    buyer_email         TEXT NOT NULL,
    -- The sale as it was struck: integer cents, VAT in basis points, the
    -- tenant's accounting currency. amount_cents = quantity * unit_price_cents,
    -- computed server-side when the order was placed.
    unit_price_cents    BIGINT NOT NULL CHECK (unit_price_cents >= 0),
    amount_cents        BIGINT NOT NULL CHECK (amount_cents >= 0),
    vat_rate_bp         INTEGER NOT NULL CHECK (vat_rate_bp >= 0),
    currency            TEXT NOT NULL,
    -- pending: placed, provider not asked yet. awaiting_payment: the provider
    -- minted a payment and the buyer is on its page. paid: money confirmed,
    -- the hold completed. failed / cancelled / expired are terminal; 'failure'
    -- carries the sentence the tenant can act on when one needs acting on
    -- (a payment that arrived after the hold lapsed names the refund).
    state               TEXT NOT NULL DEFAULT 'pending'
                        CHECK (state IN ('pending', 'awaiting_payment', 'paid',
                                         'failed', 'cancelled', 'expired')),
    provider_payment_id TEXT,
    checkout_url        TEXT,
    failure             TEXT,
    paid_at             TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL,
    updated_at          TIMESTAMPTZ NOT NULL,
    CONSTRAINT site_ticket_orders_site_fk
        FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites(tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT site_ticket_orders_event_fk
        FOREIGN KEY (tenant_id, event_id)
        REFERENCES site_ticket_events(tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT site_ticket_orders_one_per_hold UNIQUE (tenant_id, hold_id),
    CONSTRAINT site_ticket_orders_tenant_scoped UNIQUE (tenant_id, id)
);

-- The webhook's door: a provider payment id names exactly one order, across
-- all tenants — the row itself then names the tenant it belongs to.
CREATE UNIQUE INDEX site_ticket_orders_one_payment
    ON site_ticket_orders (provider_payment_id)
    WHERE provider_payment_id IS NOT NULL;

CREATE INDEX site_ticket_orders_by_site
    ON site_ticket_orders (tenant_id, site_id, created_at DESC, id);
