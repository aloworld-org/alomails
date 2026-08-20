-- alo Orders (ADR 0054 §5, item O1.c): a quote line may name the catalog item
-- it is selling.
--
-- WHY THIS IS NEEDED AT ALL. Accepting a quote is supposed to raise a sales
-- order when the offer is for goods, and a draft invoice when it is for
-- services. A quote line could not say which it was: 0105 gave it description,
-- unit, quantity, price and rate, and nothing that names a product. Worse, an
-- order copied from such lines would name no product on any line, and
-- `inv_so_deliver` refuses exactly those ("a charge in words, not goods;
-- nothing leaves against it") — so the order could never deliver anything and
-- would be an ornament. Both halves of the item were blocked on this column.
--
-- WHY IT DOES NOT WEAKEN 0105's REASONING. That migration says lines SNAPSHOT
-- the price list, with no foreign key back to `billing_products`, so a later
-- price change never rewrites an offer already made. **That stays exactly
-- true.** `description`, `unit`, `unit_price_cents` and `vat_rate_bp` are still
-- the frozen copy, and nothing reads the product to price a line. The product
-- is *provenance* — which of our items this line is — and it is the same
-- distinction migration 0700 drew for the order-to-quote link.
--
-- It is the shape `inv_sales_order_lines` has carried since 0162: nullable,
-- composite-keyed to the tenant, and SET NULL when a product is deleted from
-- the catalog, so an offer already sent stays readable exactly as it was agreed
-- even after the item behind it is gone.
--
-- Expand-only: one nullable column and one foreign key. Every existing line
-- reads as a charge in words, which is what they all were.

ALTER TABLE billing_quote_lines
    ADD COLUMN product_id TEXT;

-- The composite reference pins the product to the SAME tenant at the database
-- level: even a bug in a WHERE clause cannot put another tenant's item on our
-- offer. ON DELETE SET NULL on the product alone — deleting a catalog item must
-- never delete a line of a document a customer holds.
ALTER TABLE billing_quote_lines
    ADD CONSTRAINT billing_quote_lines_product_fk
    FOREIGN KEY (tenant_id, product_id)
        REFERENCES billing_products (tenant_id, id) ON DELETE SET NULL (product_id);
