-- alo Orders (ADR 0054 §4, item O1.b): the order an accepted quote produces.
--
-- A quote for goods becomes a sales order; a quote for services becomes an
-- invoice draft, which `billing_invoices.quote_id` has recorded since migration
-- 0106. This is that same link for the other branch, so "was this offer taken
-- up, and as what?" is answerable from either side whichever way the acceptance
-- went.
--
-- WHY A COLUMN AND NOT A LINK TABLE. An order comes from at most one quote, and
-- the invoice side already answers the identical question with a column. A
-- two-sided link table was drafted for this item by an earlier iteration and is
-- deliberately not used: a second shape for one relationship is a second thing
-- to read, and the many-to-many that ADR 0053 worried about is order-to-INVOICE,
-- which `inv_so_invoice` already handles on its own terms.
--
-- WHY THE COLUMN LIVES ON THE ORDER, not a `sales_order_id` on the quote: 0106's
-- reasoning applies unchanged. The order is the newer document and the one that
-- knows its own origin, and a column on the quote would have to be written into
-- a row that is frozen the moment it is sent — which is exactly the property
-- that makes a sent quote trustworthy.
--
-- Expand-only: one nullable column, one foreign key, one partial index. No
-- existing row can violate any of it, because nothing has ever been written
-- with an order-to-quote link. Nothing is dropped and nothing is rewritten.

ALTER TABLE inv_sales_orders
    ADD COLUMN quote_id TEXT;

-- The composite reference pins the quote to the SAME tenant at the database
-- level, exactly as `customer_id` is pinned on this table: even a bug in a
-- WHERE clause cannot link an order to another tenant's offer.
--
-- NO ACTION (the default) rather than CASCADE or SET NULL, for 0106's reason:
-- only a DRAFT quote is ever deleted and a draft has never been accepted, so no
-- linked quote can be removed while the order stands. CASCADE would be actively
-- wrong — it would delete a sales order, which is a document a customer holds a
-- number for — and dropping a tenant still works, because NO ACTION is checked
-- after the whole cascade rather than row by row.
ALTER TABLE inv_sales_orders
    ADD CONSTRAINT inv_sales_orders_quote_fk
    FOREIGN KEY (tenant_id, quote_id)
        REFERENCES billing_quotes (tenant_id, id);

-- One order per accepted quote, ever. Acceptance is a terminal transition
-- (`accepted` has no successor), so the store can produce at most one — and this
-- index is what makes "the order taken from this offer" a single row a reader
-- can rely on rather than a list it has to disambiguate. Postgres allows many
-- NULLs in a unique index, so the orders that come from no quote at all — every
-- one taken over a counter or a telephone — never collide.
CREATE UNIQUE INDEX inv_sales_orders_from_quote
    ON inv_sales_orders (tenant_id, quote_id)
    WHERE quote_id IS NOT NULL;
