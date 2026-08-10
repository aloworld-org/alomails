-- alo Inventory (ADR 0035, wave B5.05a): the purchase order — what we asked a
-- supplier for (docs/design/inventory.md, "Purchase orders").
--
-- The order is the SAME kind of object a quote is (0105): a draft that is
-- freely editable and carries no number, then a document that has left the
-- building and is frozen. What differs is who the counterparty is (a supplier,
-- not a customer), and that its terminal states are reached by goods arriving
-- rather than by an answer. Sending it — B5.05a2 — draws the number and stamps
-- `ordered_date`; receiving it — B5.05b — moves stock and closes it. Both
-- transitions are guarded here by CHECKs rather than trusted to the code that
-- will write them.
--
-- Three decisions this file records rather than assumes.
--
-- 1. THE NUMBER IS NULL UNTIL THE ORDER IS SENT, and after that it exists for
--    good. A draft nobody sent must not consume a number the supplier could
--    quote back at us. Cancelling is the one state where either is true: a
--    draft cancelled before it was ever placed has no number and never will,
--    while a sent order that is cancelled keeps the one it was sent under.
-- 2. THE LINES SNAPSHOT the price list, exactly as a billing line does
--    (billing_line.rs): no foreign key to `inv_supplier_products`, so
--    re-negotiating with a supplier never rewrites an order already placed.
--    A line MAY name one of the tenant's products — the composite key holds it
--    to this tenant — and a line that does is the one receiving will move into
--    stock (B5.05b). A line that does not is a charge in words (freight,
--    packaging), which is why the reference is nullable and why a negative
--    quantity is refused on a product line but allowed on a free-text one.
-- 3. MONEY IS INTEGER CENTS, quantities are milli-units and VAT rates basis
--    points — the same three units every document in this codebase uses, and
--    the same bounds (|qty| ≤ 10^9 × price ≤ 10^9 × 500 lines) that keep the
--    totals arithmetic inside i64.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE inv_purchase_orders (
    tenant_id     TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id            TEXT NOT NULL,
    -- Who we are buying from. The composite reference pins the supplier to the
    -- SAME tenant at the database level, so even a bug in a WHERE clause
    -- cannot link across tenants. No ON DELETE: a supplier is archived, never
    -- deleted, precisely so an order that names them stays explainable.
    supplier_id   TEXT NOT NULL,
    -- draft → sent → partially_received → received, and cancelled from draft,
    -- sent or (with a deliberate short-close) partially_received. Only a draft
    -- is editable, and only a draft is deleted.
    status        TEXT NOT NULL DEFAULT 'draft',
    -- ISO 4217, snapshotted from the supplier when the draft is raised: what
    -- THEY quote in. A later change to their default cannot restate an order.
    currency      TEXT NOT NULL DEFAULT 'EUR',
    -- PO-YYYY-NNNNN, drawn from `billing_sequences` (kind 'purchase_order')
    -- when the order is SENT. NULL while it is a draft, and on a draft that
    -- was cancelled without ever being placed.
    number        TEXT,
    -- The day we asked them, stamped with the number by the same transaction.
    ordered_date  DATE,
    -- When we expect the goods. The tenant's own expectation, editable while
    -- the order is a draft; NULL means nobody has said. Deliberately NOT
    -- derived from the supplier's lead time here — an arrival date for an
    -- order that has not been placed would be a date about nothing.
    expected_date DATE,
    -- The day the order reached a terminal state: fully received, or
    -- cancelled. The answer to "when did this stop being open?", which
    -- `updated_at` cannot give (any later touch moves that).
    closed_date   DATE,
    -- Our own reference for the order (a project code, their quotation
    -- number), and a free-text note printed on the document.
    reference     TEXT NOT NULL DEFAULT '',
    note          TEXT NOT NULL DEFAULT '',
    created_by    TEXT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, supplier_id)
        REFERENCES inv_suppliers (tenant_id, id),
    -- Defence in depth: the store validates all of this before writing, so a
    -- violation here means a bug in our code, not bad user input.
    CONSTRAINT inv_purchase_orders_status_known
        CHECK (status IN ('draft', 'sent', 'partially_received', 'received',
                          'cancelled')),
    -- A draft has neither number nor order date: sending assigns both.
    CONSTRAINT inv_purchase_orders_draft_unnumbered
        CHECK (status <> 'draft' OR (number IS NULL AND ordered_date IS NULL)),
    -- An order that has been placed has both, and no later transition clears
    -- them. `cancelled` is outside this rule in both directions, because a
    -- draft may be cancelled before it was ever placed.
    CONSTRAINT inv_purchase_orders_placed_numbered
        CHECK (status NOT IN ('sent', 'partially_received', 'received')
               OR (number IS NOT NULL AND ordered_date IS NOT NULL)),
    -- A closing date exists exactly when the order is closed.
    CONSTRAINT inv_purchase_orders_closed_iff_terminal
        CHECK ((status IN ('received', 'cancelled')) = (closed_date IS NOT NULL)),
    -- An order can never close before it was placed.
    CONSTRAINT inv_purchase_orders_closed_after_ordered
        CHECK (closed_date IS NULL OR ordered_date IS NULL
               OR closed_date >= ordered_date),
    CONSTRAINT inv_purchase_orders_currency_shape
        CHECK (currency ~ '^[A-Z]{3}$')
);

-- One number per tenant, ever. Postgres allows many NULLs in a unique index,
-- so unnumbered drafts never collide with each other.
CREATE UNIQUE INDEX inv_purchase_orders_number_unique
    ON inv_purchase_orders (tenant_id, number);
-- "Everything we ordered from them" and the status-filtered list surface.
CREATE INDEX inv_purchase_orders_by_supplier
    ON inv_purchase_orders (tenant_id, supplier_id, created_at DESC);
CREATE INDEX inv_purchase_orders_by_status
    ON inv_purchase_orders (tenant_id, status, created_at DESC);

CREATE TABLE inv_purchase_order_lines (
    tenant_id        TEXT NOT NULL,
    po_id            TEXT NOT NULL,
    id               TEXT NOT NULL,
    -- Position on the printed order, 0-based and contiguous: the store
    -- replaces the whole line set in one transaction rather than patching
    -- individual rows, so the order is always exactly the caller's order.
    line_order       INTEGER NOT NULL,
    -- What we are ordering, when it is something in the catalog. NULL for a
    -- line that is a charge in words. ON DELETE SET NULL rather than CASCADE:
    -- deleting a product must never delete the record of having ordered it —
    -- the description, quantity and price stay, exactly as they were agreed.
    product_id       TEXT,
    -- SNAPSHOTS, like every document line since B1.06: the description, unit,
    -- price and rate as they stood when the line was drafted.
    description      TEXT NOT NULL,
    unit             TEXT NOT NULL DEFAULT '',
    -- Quantity in milli-units: 1.5 kg = 1500. Positive on a product line — it
    -- becomes a movement into stock — and free to be negative on a free-text
    -- line, which is how a supplier's discount is written. The store owns that
    -- rule; this bound is only the arithmetic's.
    qty_milli        BIGINT NOT NULL,
    -- What THEY charge us per unit, copied from their price list at the moment
    -- the line was drafted.
    unit_price_cents BIGINT NOT NULL,
    vat_rate_bp      INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, id),
    -- No tenants(id) reference of its own: a line reaches its tenant only
    -- through its order, which is the single place that link is stated.
    FOREIGN KEY (tenant_id, po_id)
        REFERENCES inv_purchase_orders (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, product_id)
        REFERENCES billing_products (tenant_id, id) ON DELETE SET NULL (product_id),
    CONSTRAINT inv_purchase_order_lines_description_shape
        CHECK (length(btrim(description)) > 0),
    CONSTRAINT inv_purchase_order_lines_order_range CHECK (line_order >= 0),
    -- The bounds that keep every total inside i64 (see billing_line.rs):
    -- |qty| ≤ 10^9 milli-units × price ≤ 10^9 cents × 500 lines.
    CONSTRAINT inv_purchase_order_lines_qty_range
        CHECK (qty_milli >= -1000000000 AND qty_milli <= 1000000000),
    CONSTRAINT inv_purchase_order_lines_price_range
        CHECK (unit_price_cents >= 0 AND unit_price_cents <= 1000000000),
    CONSTRAINT inv_purchase_order_lines_vat_rate_range
        CHECK (vat_rate_bp >= 0 AND vat_rate_bp <= 10000)
);

-- The document read: every line of one order, in print order.
CREATE UNIQUE INDEX inv_purchase_order_lines_in_order
    ON inv_purchase_order_lines (tenant_id, po_id, line_order);
-- "What is on order for this product" — the open-quantity question B5.07's
-- shortage report asks, and the join receiving walks (B5.05b).
CREATE INDEX inv_purchase_order_lines_by_product
    ON inv_purchase_order_lines (tenant_id, product_id)
    WHERE product_id IS NOT NULL;
