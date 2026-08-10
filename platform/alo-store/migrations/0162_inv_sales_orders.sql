-- alo Inventory (ADR 0035, wave B5.06a): the sales order — what a customer
-- asked US for, and the deliveries that take it out of the warehouse
-- (docs/design/inventory.md, "Sales orders").
--
-- It is the purchase order mirrored, and mirrored deliberately rather than
-- generalised: the two documents share a shape, not a table. A purchase order
-- names a supplier and ends in goods ARRIVING; a sales order names a customer
-- and ends in goods LEAVING, against a promise we made rather than one we were
-- given. One table with a direction flag would make every read of either
-- document carry a WHERE clause whose omission is a silent leak between the two
-- halves of a business.
--
-- Four decisions this file records rather than assumes.
--
-- 1. THE NUMBER IS NULL UNTIL THE ORDER IS CONFIRMED, and after that it exists
--    for good — the purchase order's rule (0160), with one further reason: the
--    number goes on the delivery note that travels in the box, so a document
--    nobody has committed to must not have one.
-- 2. CONFIRMING MOVES NO STOCK AND RESERVES NOTHING. It changes what the
--    shortage query counts (B5.07) and nothing else: a sales order is a
--    promise, and goods move when they are picked. There is therefore no
--    reserved-quantity column here to drift out of step with the ledger.
-- 3. THE LINES SNAPSHOT the catalog, exactly as a billing line does: no foreign
--    key to a price, so re-pricing a product never restates an order a customer
--    already holds. A line MAY name one of the tenant's products — the
--    composite key holds it to this tenant — and a line that does is the one a
--    delivery moves out of stock. A line that does not is a charge in words
--    (delivery, assembly), which is why the reference is nullable and why a
--    negative quantity is refused on a product line but allowed on a free-text
--    one.
-- 4. THE DELIVERED QUANTITY IS AN ACCUMULATOR ON THE ORDERED LINE, not a fold
--    over the movement ledger — 0161's decision, for its reason: two lines of
--    one order may name the same product, and the ledger could then not say
--    which line a movement belongs to.
--
-- Money is integer cents, quantities milli-units, VAT rates basis points, with
-- the same bounds (|qty| ≤ 10^9 × price ≤ 10^9 × 500 lines) that keep the
-- totals arithmetic inside i64.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE inv_sales_orders (
    tenant_id      TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id             TEXT NOT NULL,
    -- Who we are selling to. The composite reference pins the customer to the
    -- SAME tenant at the database level, so even a bug in a WHERE clause cannot
    -- link across tenants. No ON DELETE: a customer is archived, never deleted,
    -- precisely so an order that names them stays explainable.
    customer_id    TEXT NOT NULL,
    -- draft → confirmed → partially_delivered → delivered, and cancelled from
    -- draft, confirmed or (with a deliberate short-close) partially_delivered.
    -- Only a draft is editable, and only a draft is deleted.
    status         TEXT NOT NULL DEFAULT 'draft',
    -- ISO 4217, snapshotted from the customer when the draft is raised: what WE
    -- quote them in. A later change to their default cannot restate an order.
    currency       TEXT NOT NULL DEFAULT 'EUR',
    -- SO-YYYY-NNNNN, drawn from `billing_sequences` (kind 'sales_order') when
    -- the order is CONFIRMED. NULL while it is a draft, and on a draft that was
    -- cancelled without ever being confirmed.
    number         TEXT,
    -- The day we committed to it, stamped with the number by the same
    -- transaction.
    confirmed_date DATE,
    -- The day we promised the goods. Our own promise, editable while the order
    -- is a draft; NULL means nobody has said.
    expected_date  DATE,
    -- The day the order reached a terminal state: fully delivered, or
    -- cancelled. The answer to "when did this stop being open?", which
    -- `updated_at` cannot give (any later touch moves that).
    closed_date    DATE,
    -- The customer's own reference for the order (their PO number, a project
    -- code), and a free-text note printed on the document.
    reference      TEXT NOT NULL DEFAULT '',
    note           TEXT NOT NULL DEFAULT '',
    created_by     TEXT NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, customer_id)
        REFERENCES billing_customers (tenant_id, id),
    -- Defence in depth: the store validates all of this before writing, so a
    -- violation here means a bug in our code, not bad user input.
    CONSTRAINT inv_sales_orders_status_known
        CHECK (status IN ('draft', 'confirmed', 'partially_delivered', 'delivered',
                          'cancelled')),
    -- A draft has neither number nor confirmation date: confirming assigns both.
    CONSTRAINT inv_sales_orders_draft_unnumbered
        CHECK (status <> 'draft' OR (number IS NULL AND confirmed_date IS NULL)),
    -- An order we have committed to has both, and no later transition clears
    -- them. `cancelled` is outside this rule in both directions, because a
    -- draft may be cancelled before it was ever confirmed.
    CONSTRAINT inv_sales_orders_committed_numbered
        CHECK (status NOT IN ('confirmed', 'partially_delivered', 'delivered')
               OR (number IS NOT NULL AND confirmed_date IS NOT NULL)),
    -- A closing date exists exactly when the order is closed.
    CONSTRAINT inv_sales_orders_closed_iff_terminal
        CHECK ((status IN ('delivered', 'cancelled')) = (closed_date IS NOT NULL)),
    -- An order can never close before it was confirmed.
    CONSTRAINT inv_sales_orders_closed_after_confirmed
        CHECK (closed_date IS NULL OR confirmed_date IS NULL
               OR closed_date >= confirmed_date),
    CONSTRAINT inv_sales_orders_currency_shape
        CHECK (currency ~ '^[A-Z]{3}$')
);

-- One number per tenant, ever. Postgres allows many NULLs in a unique index,
-- so unnumbered drafts never collide with each other.
CREATE UNIQUE INDEX inv_sales_orders_number_unique
    ON inv_sales_orders (tenant_id, number);
-- "Everything they ordered from us" and the status-filtered list surface.
CREATE INDEX inv_sales_orders_by_customer
    ON inv_sales_orders (tenant_id, customer_id, created_at DESC);
CREATE INDEX inv_sales_orders_by_status
    ON inv_sales_orders (tenant_id, status, created_at DESC);

CREATE TABLE inv_sales_order_lines (
    tenant_id           TEXT NOT NULL,
    so_id               TEXT NOT NULL,
    id                  TEXT NOT NULL,
    -- Position on the printed order, 0-based and contiguous: the store replaces
    -- the whole line set in one transaction rather than patching individual
    -- rows, so the order is always exactly the caller's order.
    line_order          INTEGER NOT NULL,
    -- What they are buying, when it is something in the catalog. NULL for a
    -- line that is a charge in words. ON DELETE SET NULL rather than CASCADE:
    -- deleting a product must never delete the record of having sold it — the
    -- description, quantity and price stay, exactly as they were agreed.
    product_id          TEXT,
    -- SNAPSHOTS, like every document line since B1.06: the description, unit,
    -- price and rate as they stood when the line was drafted.
    description         TEXT NOT NULL,
    unit                TEXT NOT NULL DEFAULT '',
    -- Quantity in milli-units: 1.5 kg = 1500. Positive on a product line — it
    -- becomes a movement out of stock — and free to be negative on a free-text
    -- line, which is how a discount is written. The store owns that rule; this
    -- bound is only the arithmetic's.
    qty_milli           BIGINT NOT NULL,
    -- What WE charge them per unit, copied from the catalog at the moment the
    -- line was drafted.
    unit_price_cents    BIGINT NOT NULL,
    vat_rate_bp         INTEGER NOT NULL,
    -- How much of this line has left the building, in the same milli-units it
    -- was ordered in. Written only by the delivering transaction, which writes
    -- the movements in the same breath.
    delivered_qty_milli BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (tenant_id, id),
    -- No tenants(id) reference of its own: a line reaches its tenant only
    -- through its order, which is the single place that link is stated.
    FOREIGN KEY (tenant_id, so_id)
        REFERENCES inv_sales_orders (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, product_id)
        REFERENCES billing_products (tenant_id, id) ON DELETE SET NULL (product_id),
    CONSTRAINT inv_sales_order_lines_description_shape
        CHECK (length(btrim(description)) > 0),
    CONSTRAINT inv_sales_order_lines_order_range CHECK (line_order >= 0),
    -- The bounds that keep every total inside i64 (see billing_line.rs):
    -- |qty| ≤ 10^9 milli-units × price ≤ 10^9 cents × 500 lines.
    CONSTRAINT inv_sales_order_lines_qty_range
        CHECK (qty_milli >= -1000000000 AND qty_milli <= 1000000000),
    CONSTRAINT inv_sales_order_lines_price_range
        CHECK (unit_price_cents >= 0 AND unit_price_cents <= 1000000000),
    CONSTRAINT inv_sales_order_lines_vat_rate_range
        CHECK (vat_rate_bp >= 0 AND vat_rate_bp <= 10000),
    -- The over-delivery bound, phrased as `GREATEST(qty_milli, 0)` rather than
    -- in terms of `product_id` (0161's decision): a free-text line may carry a
    -- negative quantity and can never be delivered, which that expression gives
    -- for free, and phrasing it against `product_id` would re-evaluate on the
    -- ON DELETE SET NULL that deleting a catalog item performs.
    CONSTRAINT inv_sales_order_lines_delivered_range
        CHECK (delivered_qty_milli >= 0
               AND delivered_qty_milli <= GREATEST(qty_milli, 0))
);

-- The document read: every line of one order, in print order.
CREATE UNIQUE INDEX inv_sales_order_lines_in_order
    ON inv_sales_order_lines (tenant_id, so_id, line_order);
-- "What is promised out of this product" — the demand side of the shortage
-- question B5.07 asks, and the join every delivery walks.
CREATE INDEX inv_sales_order_lines_by_product
    ON inv_sales_order_lines (tenant_id, product_id)
    WHERE product_id IS NOT NULL;

CREATE TABLE inv_so_deliveries (
    tenant_id      TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id             TEXT NOT NULL,
    -- The order the goods went out against. ON DELETE CASCADE follows the
    -- lines': only a draft is ever deleted, and a draft has no deliveries.
    so_id          TEXT NOT NULL,
    -- Where they were picked FROM. Any of this tenant's locations — the store
    -- holds it to a real one, because goods cannot be picked from the customer.
    location_id    TEXT NOT NULL,
    -- 1 for the first delivery against this order, 2 for the next. The delivery
    -- note's number is built from it, and a person says "the second delivery".
    sequence_no    INTEGER NOT NULL,
    -- The day the goods left, from the database's own clock inside the
    -- delivering transaction — never the caller's.
    delivered_date DATE NOT NULL DEFAULT CURRENT_DATE,
    -- What the person who packed it wrote: "two boxes, driver Kowalski".
    note           TEXT NOT NULL DEFAULT '',
    created_by     TEXT NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, so_id)
        REFERENCES inv_sales_orders (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, location_id)
        REFERENCES inv_locations (tenant_id, id),
    CONSTRAINT inv_so_deliveries_sequence_range CHECK (sequence_no >= 1),
    CONSTRAINT inv_so_deliveries_note_length CHECK (length(note) <= 500)
);

-- One ordinal per order, ever: two deliveries entered at the same instant
-- cannot both be "the second one", and neither can number one delivery note
-- twice.
CREATE UNIQUE INDEX inv_so_deliveries_in_order
    ON inv_so_deliveries (tenant_id, so_id, sequence_no);
-- "What has gone out against this order", newest first.
CREATE INDEX inv_so_deliveries_by_order
    ON inv_so_deliveries (tenant_id, so_id, delivered_date DESC, sequence_no DESC);

CREATE TABLE inv_so_delivery_lines (
    tenant_id   TEXT NOT NULL,
    id          TEXT NOT NULL,
    delivery_id TEXT NOT NULL,
    -- Which ordered line went out. A reference, not a snapshot: the line's own
    -- words, price and rate are the order's and stay there.
    so_line_id  TEXT NOT NULL,
    -- How much went out now, in milli-units. Strictly positive: a delivery of
    -- nothing is not a delivery, and goods coming back from a customer are a
    -- return (a movement the other way), never a negative delivery.
    qty_milli   BIGINT NOT NULL,
    -- The movement this line wrote. NOT NULL: a delivery line with no movement
    -- would be stock that left for nowhere. The cascade below is not a licence
    -- to delete history — the ledger is append-only and no door removes a
    -- movement — it is what lets a whole tenant be dropped in one statement,
    -- since this table reaches `tenants` only through its delivery and would
    -- otherwise be checked before that cascade had run.
    move_id     TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, delivery_id)
        REFERENCES inv_so_deliveries (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, so_line_id)
        REFERENCES inv_sales_order_lines (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, move_id)
        REFERENCES inv_moves (tenant_id, id) ON DELETE CASCADE,
    -- The same arithmetic bound every quantity in this codebase carries.
    CONSTRAINT inv_so_delivery_lines_qty_range
        CHECK (qty_milli > 0 AND qty_milli <= 1000000000),
    -- One ordered line goes out at most once per delivery: two figures for the
    -- same line on one note is a caller that has lost track of its form.
    CONSTRAINT inv_so_delivery_lines_one_per_ordered_line
        UNIQUE (tenant_id, delivery_id, so_line_id)
);

-- "What has gone out against this ordered line" — the join every delivery read
-- walks, and the one B5.06b's invoice bridge will.
CREATE INDEX inv_so_delivery_lines_by_ordered_line
    ON inv_so_delivery_lines (tenant_id, so_line_id);
