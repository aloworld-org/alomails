-- alo Inventory (ADR 0035, wave B5.02): the catalog upgrade — five more facts
-- about a product a warehouse needs, added to the table that already owns a
-- product (docs/design/inventory.md, "The catalog").
--
-- The rejected alternative was a sibling `inv_items` keyed by product id. A
-- product is ONE thing in a tenant's head: it has one name and one SKU, and a
-- two-table split immediately raises the question of what a row in one and not
-- the other means — a question with no good answer. Extending the owner beats
-- creating a half-overlapping sibling.
--
-- Expand-only, as every migration on this track is: six columns with defaults,
-- nothing rewritten, nothing dropped. A build that has not seen this migration
-- reads and writes products exactly as before, and every product that exists
-- today stays a service (`stocked = false`) — no tenant acquires a stock
-- ledger by upgrade.

ALTER TABLE billing_products
    -- The tenant's own code for the item. Unique WITHIN the tenant when
    -- non-blank; blank is legitimate and unconstrained, because a services
    -- business has none.
    ADD COLUMN sku                  TEXT NOT NULL DEFAULT '',
    -- The code on the box — GTIN-8/12/13/14, check-digit validated in the
    -- store (`inv_barcode`). TEXT and never a number: a GTIN's leading zeros
    -- are part of it, and an integer column eats them, which is the classic
    -- bug that makes two different codes on two different boxes the same row.
    ADD COLUMN barcode              TEXT NOT NULL DEFAULT '',
    -- Whether this product has a quantity at all. Default false, so the move
    -- ledger (B5.04a) will refuse a movement of a service until somebody says
    -- otherwise: "3 hours of consulting moved from the warehouse to the van"
    -- is not a sentence this system should be able to hold.
    ADD COLUMN stocked              BOOLEAN NOT NULL DEFAULT false,
    -- What we PAY, in integer cents of the tenant's own currency. The sale
    -- price is `unit_price_cents` and stays exactly where it is.
    ADD COLUMN purchase_price_cents BIGINT NOT NULL DEFAULT 0,
    -- The product photo as a Drive node, referenced by id and never copied —
    -- the shape `fin_expenses.receipt_node_id` established (B4.05a). No
    -- foreign key, for the same two reasons: Drive nodes are not a billing
    -- dependency, and purging a file must not delete the product it pictured.
    -- The store checks the caller can read the node when it is set.
    ADD COLUMN photo_node_id        TEXT,
    -- Who we usually buy it from — the seed of a reorder proposal (B5.07).
    -- Reserved by this migration and NOT yet writable: `inv_suppliers` is
    -- B5.03's table, and the composite foreign key that makes this id
    -- necessarily the same tenant's supplier arrives with it. Until then
    -- nothing writes this column, so no dangling reference can exist.
    ADD COLUMN default_supplier_id  TEXT;

ALTER TABLE billing_products
    -- Defence in depth: the store validates all of these before writing, so a
    -- violation here means a bug in our code, not user input.
    ADD CONSTRAINT billing_products_sku_shape
        CHECK (sku = btrim(sku) AND char_length(sku) <= 64),
    -- A GTIN is digits only and comes in exactly four lengths. Blank means
    -- "no barcode", which plenty of stock genuinely has.
    ADD CONSTRAINT billing_products_barcode_shape
        CHECK (barcode ~ '^[0-9]*$'
               AND (barcode = '' OR char_length(barcode) IN (8, 12, 13, 14))),
    ADD CONSTRAINT billing_products_purchase_price_range
        CHECK (purchase_price_cents >= 0 AND purchase_price_cents <= 1000000000);

-- Both uniqueness rules are PARTIAL and TENANT-SCOPED. A global unique index
-- on a barcode would be a cross-tenant information leak of the plainest kind —
-- tenant B's insert failing because tenant A already sells the same book — and
-- it would be wrong on the facts too, since two businesses legitimately stock
-- the same GTIN. Partial, because blank is the "not stated" value here and
-- every product without an SKU would otherwise collide with every other.
CREATE UNIQUE INDEX billing_products_sku_unique
    ON billing_products (tenant_id, sku) WHERE sku <> '';
CREATE UNIQUE INDEX billing_products_barcode_unique
    ON billing_products (tenant_id, barcode) WHERE barcode <> '';

-- The scan surface (B5.09c) and the stock screens (B5.09a) ask one question of
-- this table: which products carry a quantity. Partial, because on a services
-- tenant the answer is none of them.
CREATE INDEX billing_products_stocked
    ON billing_products (tenant_id, lower(name))
    WHERE stocked AND archived_at IS NULL;
