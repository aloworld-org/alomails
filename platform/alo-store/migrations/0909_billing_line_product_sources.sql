-- Optional catalogue provenance for invoice and recurring-template snapshots.
-- The description, unit, price and VAT remain frozen on each line; this link
-- only lets development data and future detail views explain which price-list
-- item supplied that snapshot.
ALTER TABLE billing_invoice_lines
    ADD COLUMN source_product_id TEXT,
    ADD CONSTRAINT billing_invoice_lines_source_product_fk
        FOREIGN KEY (tenant_id, source_product_id)
        REFERENCES billing_products (tenant_id, id);

CREATE INDEX billing_invoice_lines_by_source_product
    ON billing_invoice_lines (tenant_id, source_product_id)
    WHERE source_product_id IS NOT NULL;

ALTER TABLE billing_schedule_lines
    ADD COLUMN source_product_id TEXT,
    ADD CONSTRAINT billing_schedule_lines_source_product_fk
        FOREIGN KEY (tenant_id, source_product_id)
        REFERENCES billing_products (tenant_id, id);

CREATE INDEX billing_schedule_lines_by_source_product
    ON billing_schedule_lines (tenant_id, source_product_id)
    WHERE source_product_id IS NOT NULL;
