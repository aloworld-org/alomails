-- alo Inventory (ADR 0035, wave B5.08a): the stocktake — a count of one place,
-- the expected quantity of everything on it at the moment counting started, and
-- what a person actually found (docs/design/inventory.md, "Stocktake").
--
-- Four decisions this file records rather than assumes.
--
-- 1. **The snapshot is a reading, not an authority.** `expected_qty_milli` is
--    what the ledger said when the sheet was opened, kept so the sheet can be
--    printed and so a counter can see what they were meant to find. It is NOT
--    what the variance is computed against when the count is applied (B5.08b):
--    a warehouse does not stop while it is counted, and applying a frozen
--    difference would silently erase a delivery that went out at the far end of
--    the room. Applying recomputes against on-hand at that moment and skips any
--    line that moved in between.
--
-- 2. **An uncounted line is uncounted, not zero.** `counted_qty_milli` is
--    nullable and starts NULL, because "nobody has looked at this shelf yet"
--    and "there are none left" are opposite facts and a stocktake that confused
--    them would write off everything nobody got to. A line nobody counts is
--    skipped when the count is applied.
--
-- 3. **One open count per location.** Two people counting the same shelf at the
--    same time produce two truths, and applying both would adjust the same
--    variance twice. The partial unique index below makes that unrepresentable;
--    a second open count for a place is a `409`, not a second sheet. Counts
--    that are finished (applied or cancelled) are outside the index, so a shelf
--    can be counted every week forever.
--
-- 4. **There is still no quantity column on a product.** A count does not edit
--    stock; applying it writes ordinary adjustment MOVEMENTS (B5.04b) like
--    every other change to the ledger, so "where did the other four go" keeps
--    its answer. This table is a worksheet, not a second source of truth.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE inv_counts (
    tenant_id   TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id          TEXT NOT NULL,
    -- Which place is being counted. A real `stock` location only: a count of
    -- the `supplier` counterparty would be a count of a number that is
    -- negative by construction, and a count of `transit` would be a count of
    -- goods that are, by definition, not anywhere to be counted.
    location_id TEXT NOT NULL,
    --   open       being counted; the only state in which lines may be written
    --   applied    the variances became adjustment movements (B5.08b)
    --   cancelled  walked away from; the sheet is kept, the ledger untouched
    -- Both closed states are terminal: a count that has been applied cannot be
    -- applied again, which is what stops one afternoon's variance being written
    -- into the ledger twice.
    status      TEXT NOT NULL DEFAULT 'open',
    -- What the person wrote about the count ("Tuesday, back shelves"). Bounded
    -- and never logged, like every other free-text field in the module.
    note        TEXT NOT NULL DEFAULT '',
    created_by  TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- When it stopped being open, and who stopped it. NULL exactly while open.
    closed_at   TIMESTAMPTZ,
    closed_by   TEXT,
    PRIMARY KEY (tenant_id, id),
    -- Composite and tenant-first: a count cannot name another tenant's location
    -- even if the store had a bug (docs/design/inventory.md, "Tenancy").
    CONSTRAINT inv_counts_location_fk FOREIGN KEY (tenant_id, location_id)
        REFERENCES inv_locations (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT inv_counts_status_shape
        CHECK (status IN ('open', 'applied', 'cancelled')),
    CONSTRAINT inv_counts_note_length CHECK (length(note) <= 500),
    -- A count is closed exactly when it is not open, and a closing is always
    -- somebody's act: the three columns cannot drift apart.
    CONSTRAINT inv_counts_closed_with_status
        CHECK ((status = 'open') = (closed_at IS NULL)),
    CONSTRAINT inv_counts_closed_by_whom
        CHECK ((closed_at IS NULL) = (closed_by IS NULL))
);

-- **One open count per place** — decision 3 above, made unrepresentable rather
-- than merely refused in the store.
CREATE UNIQUE INDEX inv_counts_one_open_per_location
    ON inv_counts (tenant_id, location_id)
    WHERE status = 'open';

-- The stocktake list: this tenant's counts, newest first, optionally for one
-- place.
CREATE INDEX inv_counts_by_location
    ON inv_counts (tenant_id, location_id, created_at DESC);

CREATE TABLE inv_count_lines (
    tenant_id          TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    count_id           TEXT NOT NULL,
    -- The product on the shelf. The pair (count, product) IS the line's
    -- identity — a sheet with the same item on two rows is a sheet somebody has
    -- to add up by hand — which is why a line needs no id of its own and is
    -- addressed as `…/counts/{id}/lines/{product_id}`.
    product_id         TEXT NOT NULL,
    -- On-hand at the moment this line joined the sheet: at the snapshot for the
    -- rows the count opened with, at the `PUT` for a row a counter added by
    -- scanning something the sheet did not expect. A reading (decision 1).
    expected_qty_milli BIGINT NOT NULL,
    -- What was actually found. NULL until somebody looks (decision 2).
    counted_qty_milli  BIGINT,
    -- Who looked, and when. NULL exactly while the line is uncounted; a line
    -- can be un-counted again, which clears all three together (undo over
    -- confirm, docs/design/ux-principles.md).
    counted_at         TIMESTAMPTZ,
    counted_by         TEXT,
    -- What the counter wrote about this row ("two boxes water-damaged").
    note               TEXT NOT NULL DEFAULT '',
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, count_id, product_id),
    CONSTRAINT inv_count_lines_count_fk FOREIGN KEY (tenant_id, count_id)
        REFERENCES inv_counts (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT inv_count_lines_product_fk FOREIGN KEY (tenant_id, product_id)
        REFERENCES billing_products (tenant_id, id) ON DELETE CASCADE,
    -- Defence in depth: the store validates all of this before writing, so a
    -- violation here means a bug in our code, not user input. The cap is the
    -- one a movement carries (`inv_moves`, B5.04a), because a variance is a
    -- movement waiting to be written; the floor is zero because a shelf cannot
    -- hold minus four of anything, and a count is a statement about a shelf.
    CONSTRAINT inv_count_lines_expected_range
        CHECK (expected_qty_milli >= 0 AND expected_qty_milli <= 1000000000),
    CONSTRAINT inv_count_lines_counted_range
        CHECK (counted_qty_milli IS NULL
               OR (counted_qty_milli >= 0 AND counted_qty_milli <= 1000000000)),
    CONSTRAINT inv_count_lines_counted_together
        CHECK ((counted_qty_milli IS NULL) = (counted_at IS NULL)
               AND (counted_at IS NULL) = (counted_by IS NULL)),
    CONSTRAINT inv_count_lines_note_length CHECK (length(note) <= 500)
);

-- "Has this product been counted lately?" — the product drawer's read, and the
-- join B5.08b's apply makes per line.
CREATE INDEX inv_count_lines_by_product
    ON inv_count_lines (tenant_id, product_id);
