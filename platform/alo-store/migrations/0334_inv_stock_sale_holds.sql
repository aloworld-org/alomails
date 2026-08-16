-- The stock-sale hold — a web shop's reservation against the warehouse's own
-- count (migration 0334, ADR 0041, item S3.05a1).
--
-- Stock is one number (ADR 0041): the shelf count lives in inv_stock, written
-- only by the move ledger, and this table deliberately stores NO quantity of
-- stock — only quantities *reserved from* it, for minutes, while a buyer is
-- paying. Available-to-sell is computed at every read as the ledger's on-hand
-- minus the live rows here; there is no cached availability anywhere to drift.
--
-- The hold's life mirrors the ticket hold (0329): held until it completes,
-- releases or expires, with expiry a time predicate rather than a sweeper.
-- One difference is deliberate and load-bearing: a COMPLETED ticket hold
-- counts against capacity forever, but a completed stock hold counts for
-- NOTHING — completing it records the real outbound movement in inv_moves in
-- the same transaction, so the shelf count itself has already dropped and
-- counting the hold too would subtract the sale twice.
--
-- Like the ticket hold, this is pure quantity accounting: no buyer identity
-- of any kind, proven by the columns-of-the-table test in
-- tests/inv_stock_sale.rs. Who bought lives where the sale puts it (the
-- order, the invoice, the CRM card — S3.05a2), in records the tenant owns.
--
-- No ON DELETE action on the product on purpose (0157's choice): a product
-- with a live reservation cannot be deleted out from under it, exactly as a
-- product with movements cannot.

CREATE TABLE inv_stock_sale_holds (
    id           TEXT PRIMARY KEY,
    tenant_id    TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    product_id   TEXT NOT NULL,
    qty_milli    BIGINT NOT NULL CHECK (qty_milli > 0),
    state        TEXT NOT NULL DEFAULT 'held'
                 CHECK (state IN ('held', 'completed', 'released', 'expired')),
    expires_at   TIMESTAMPTZ NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    CONSTRAINT inv_stock_sale_holds_product_fk
        FOREIGN KEY (tenant_id, product_id)
        REFERENCES billing_products (tenant_id, id),
    CONSTRAINT inv_stock_sale_holds_tenant_scoped UNIQUE (tenant_id, id)
);

-- The availability read's index: the live holds of one product, by expiry.
CREATE INDEX inv_stock_sale_holds_live
    ON inv_stock_sale_holds (tenant_id, product_id, expires_at)
    WHERE state = 'held';
