-- alo Inventory (ADR 0035, wave B5.03): the suppliers a tenant buys from, and
-- the price list THEY quote US (docs/design/inventory.md, "Suppliers").
--
-- Two decisions this file records rather than assumes.
--
-- 1. A supplier is its OWN table, not a flag on `billing_customers`. The two
--    records genuinely overlap — name, address, VAT id, country — and a single
--    flagged "company" table is a design real products ship. It is rejected
--    because the fields diverge immediately (a customer has payment terms *we*
--    grant; a supplier has lead times, their own code for our products, and an
--    IBAN we pay into) and because the failure mode of a wrong flag is putting
--    a supplier in the customer picker of an invoice. Two tables cannot make
--    that mistake.
-- 2. `billing_bills` keeps its COPIED supplier (name, VAT id, address, IBAN)
--    and gains NO foreign key to this table. A bill must stay readable exactly
--    as it arrived, years later, whatever has since happened to the master
--    record — the snapshot rule every document line has held since B1.06.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE inv_suppliers (
    tenant_id          TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id                 TEXT NOT NULL,
    -- Who they are. Required — a supplier without a name is a row nobody can
    -- read on an order.
    name               TEXT NOT NULL,
    address_line1      TEXT NOT NULL DEFAULT '',
    address_line2      TEXT NOT NULL DEFAULT '',
    postal_code        TEXT NOT NULL DEFAULT '',
    city               TEXT NOT NULL DEFAULT '',
    -- ISO 3166-1 alpha-2, uppercased in the store. Required for the same
    -- reason it is on a customer: it decides which member state's rules their
    -- VAT id is held to, and whether a purchase is reverse-charged.
    country            TEXT NOT NULL,
    -- VAT identification number, NULL when they have not given one. Stored in
    -- the canonical prefixed form the B1.03 validator produces.
    vat_id             TEXT,
    -- Company/registration number as printed on their paper. Free text: every
    -- member state numbers companies its own way.
    registration_no    TEXT NOT NULL DEFAULT '',
    -- Where a purchase order is sent (B5.05a drafts the mail); NULL when
    -- unknown, and sending an order to a supplier without one is refused there.
    email              TEXT,
    phone              TEXT NOT NULL DEFAULT '',
    -- The account we pay INTO, mod-97 checked in the store (`crate::iban`).
    -- NULL when they have not given one.
    iban               TEXT,
    -- ISO 4217, uppercased. What THEY quote in; a purchase order copies it at
    -- the moment it is drafted, exactly as an invoice copies the customer's.
    currency           TEXT NOT NULL DEFAULT 'EUR',
    -- Days from their invoice date to when we owe them.
    payment_terms_days INTEGER NOT NULL DEFAULT 30,
    -- How long, in days, between ordering and the goods arriving. The default
    -- for every product they sell us; one offer may override it below.
    lead_time_days     INTEGER NOT NULL DEFAULT 0,
    -- The tenant's own note about the relationship. Never logged (Law 1).
    note               TEXT NOT NULL DEFAULT '',
    -- NULL = active. Archiving stops us buying from them without deleting the
    -- orders that name them.
    archived_at        TIMESTAMPTZ,
    created_by         TEXT NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- Defence in depth: the store validates every one of these before writing,
    -- so a violation here means a bug in our code, not user input.
    CONSTRAINT inv_suppliers_name_shape CHECK (length(btrim(name)) > 0),
    CONSTRAINT inv_suppliers_country_shape CHECK (country ~ '^[A-Z]{2}$'),
    CONSTRAINT inv_suppliers_currency_shape CHECK (currency ~ '^[A-Z]{3}$'),
    CONSTRAINT inv_suppliers_terms_range
        CHECK (payment_terms_days >= 0 AND payment_terms_days <= 365),
    CONSTRAINT inv_suppliers_lead_time_range
        CHECK (lead_time_days >= 0 AND lead_time_days <= 365)
);

-- The list surface: a tenant's suppliers in name order, active ones first.
CREATE INDEX inv_suppliers_by_name ON inv_suppliers (tenant_id, lower(name));

-- What one supplier quotes us for one product. Keyed by the pair, so a
-- second quote for the same product REPLACES the first (the `PUT` the design
-- note calls idempotent) rather than growing a history nobody asked for.
--
-- Prices here are a REFERENCE, not a snapshot: a purchase-order line copies
-- the price at the moment it is drafted (B5.05a), the same rule a billing line
-- holds about the sale price, so re-negotiating never rewrites an order that
-- was already placed.
CREATE TABLE inv_supplier_products (
    tenant_id            TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    supplier_id          TEXT NOT NULL,
    product_id           TEXT NOT NULL,
    -- Their article code for our product — what goes on the order so their
    -- warehouse picks the right thing.
    supplier_code        TEXT NOT NULL DEFAULT '',
    -- What they charge us for one unit, in integer cents of `currency`.
    -- Never a float, like every other money column in this codebase.
    purchase_price_cents BIGINT NOT NULL DEFAULT 0,
    currency             TEXT NOT NULL DEFAULT 'EUR',
    -- The smallest quantity they will sell, in MILLI-units — the same
    -- thousandth-precision quantity a document line carries (B1.06), so
    -- "0.5 kg" is 500 and no fraction is ever a float.
    min_order_qty_milli  BIGINT NOT NULL DEFAULT 0,
    -- Overrides the supplier's default lead time for this product only.
    -- NULL means "as the supplier says", which is the common case.
    lead_time_days       INTEGER,
    created_by           TEXT NOT NULL,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, supplier_id, product_id),
    -- Both legs composite and tenant-first: an offer cannot name another
    -- tenant's supplier or another tenant's product even if the store had a
    -- bug (docs/design/inventory.md, "Tenancy").
    CONSTRAINT inv_supplier_products_supplier_fk FOREIGN KEY (tenant_id, supplier_id)
        REFERENCES inv_suppliers (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT inv_supplier_products_product_fk FOREIGN KEY (tenant_id, product_id)
        REFERENCES billing_products (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT inv_supplier_products_currency_shape CHECK (currency ~ '^[A-Z]{3}$'),
    CONSTRAINT inv_supplier_products_price_range
        CHECK (purchase_price_cents >= 0 AND purchase_price_cents <= 1000000000),
    CONSTRAINT inv_supplier_products_min_qty_range
        CHECK (min_order_qty_milli >= 0 AND min_order_qty_milli <= 1000000000000),
    CONSTRAINT inv_supplier_products_lead_time_range
        CHECK (lead_time_days IS NULL
               OR (lead_time_days >= 0 AND lead_time_days <= 365))
);

-- "Who sells us this?" — the read the reorder proposal (B5.07) makes per
-- shortage, and the one a product drawer makes to show its offers.
CREATE INDEX inv_supplier_products_by_product
    ON inv_supplier_products (tenant_id, product_id);

-- B5.02 reserved `billing_products.default_supplier_id` and deliberately left
-- nothing writing it: the composite key that makes the id NECESSARILY the same
-- tenant's supplier could only arrive with the supplier table. It arrives now,
-- and the write path arrives with it.
--
-- SET NULL names its column explicitly (PostgreSQL 15+): the plain form would
-- try to null `tenant_id` too, which is NOT NULL and part of the key. In
-- practice a supplier is archived rather than deleted, so this fires only when
-- a whole tenant goes.
ALTER TABLE billing_products
    ADD CONSTRAINT billing_products_default_supplier_fk
        FOREIGN KEY (tenant_id, default_supplier_id)
        REFERENCES inv_suppliers (tenant_id, id)
        ON DELETE SET NULL (default_supplier_id);
