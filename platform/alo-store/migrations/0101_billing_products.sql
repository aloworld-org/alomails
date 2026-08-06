-- alo Billing (ADR 0035, wave B1): the tenant's price list.
--
-- Tenant-scoped and cascading with the tenant (Law 1), tenant-wide like
-- `billing_customers`: every user of the tenant sells from the same list.
-- Products are ARCHIVED, never deleted — a document line snapshots the price
-- and rate it was raised with (docs/design/billing.md), so nothing here is a
-- dependency of an issued invoice, but a discontinued item must still be
-- explainable to whoever reads last year's books.
--
-- Money is integer cents and VAT rates are basis points; no column in alo
-- Billing is ever floating point.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE billing_products (
    tenant_id        TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id               TEXT NOT NULL,
    -- What appears as the line description when the product is picked.
    name             TEXT NOT NULL,
    -- Free-text unit label ("hour", "piece", "kg"); blank for a unitless
    -- item. EN 16931 wants a UN/ECE Rec 20 code instead — that mapping
    -- belongs to the e-invoice writer (B1.22), not to the price list.
    unit             TEXT NOT NULL DEFAULT '',
    -- Price for one unit, in integer cents of the tenant's default currency.
    -- The document, not the price list, carries the currency it was raised
    -- in (billing_invoices, B1.06) and its FX snapshot (B1.21).
    unit_price_cents BIGINT NOT NULL DEFAULT 0,
    -- VAT rate in basis points: 2100 = 21 %. Zero is legitimate (exempt,
    -- reverse charge, intra-Community supply).
    vat_rate_bp      INTEGER NOT NULL DEFAULT 0,
    -- NULL = active. Archiving hides the product from pickers without
    -- rewriting any document that was raised from it.
    archived_at      TIMESTAMPTZ,
    created_by       TEXT NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- Defence in depth: the store validates all three before writing, so a
    -- violation here means a bug, not user input.
    CONSTRAINT billing_products_name_shape CHECK (length(btrim(name)) > 0),
    CONSTRAINT billing_products_price_range
        CHECK (unit_price_cents >= 0 AND unit_price_cents <= 1000000000),
    CONSTRAINT billing_products_vat_rate_range
        CHECK (vat_rate_bp >= 0 AND vat_rate_bp <= 10000)
);

-- The list surface: a tenant's price list in name order, active ones first.
CREATE INDEX billing_products_by_name ON billing_products (tenant_id, lower(name));
