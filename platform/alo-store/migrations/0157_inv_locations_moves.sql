-- alo Inventory (ADR 0035, wave B5.04a): the places stock can be, the ledger
-- of every movement between them, and the cached balance that ledger implies
-- (docs/design/inventory.md, "Locations and the move ledger").
--
-- The one decision this file exists to record: **there is no quantity column
-- on a product**. On-hand is derived from movements, the way a balance is
-- derived from postings, because a quantity edited in place cannot answer the
-- only question a warehouse ever really asks — "where did the other four go?".
-- `inv_stock` below is a CACHE of that fold and never a second source of
-- truth: one writer (`alo_store::inv_moves::record_move`), in the same
-- transaction as the movement it summarises, proven against a recomputed fold
-- by a property test after every write.
--
-- The second decision: **both ends of a movement are real rows**. Goods
-- received come FROM a `supplier` location, goods delivered go TO a `customer`
-- one, a stocktake variance moves to or from `adjustment`. The rejected
-- alternative — nullable columns meaning "from outside" — makes every SUM
-- remember the null and makes the invariant that guards this whole module
-- ("every quantity sums to zero across all locations") unstatable. This is the
-- argument B4.03a made for booking against a real account rather than "and the
-- other side went somewhere".
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE inv_locations (
    tenant_id   TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id          TEXT NOT NULL,
    -- What a person types and what a printed count sheet says. Uppercased and
    -- space-free in the store, for the reason an account code is: `wh1` and
    -- `WH1` are the same place to every human reading a shelf label, and
    -- storing both produces two rows a stock report shows as separate lines.
    code        TEXT NOT NULL,
    -- The reader's own word for the place. Seeded per tenant in the language
    -- of whoever opened Inventory first, exactly as the chart of accounts is
    -- (B4.02): a warehouse called 'Warehouse' in a Dutch tenant is a hardcoded
    -- English string in a European product.
    name        TEXT NOT NULL,
    -- The load-bearing column. Two values a person picks:
    --   stock      a real place — warehouse, shop floor, van. On-hand here is
    --              a claim about physical goods and may never go negative.
    --   transit    real too, but nobody counts it: goods that have left one
    --              location and not yet arrived at another.
    -- …and four the system owns, seeded once per tenant, neither creatable nor
    -- deletable through any door:
    --   supplier   where received goods come from
    --   customer   where delivered goods go
    --   adjustment the counterparty of every correction and stocktake variance
    --   production seeded and unused in B5 (assembly is a stated cut), so the
    --              day it is needed there is no migration and no new kind
    kind        TEXT NOT NULL,
    -- NULL = active. A location that has carried a movement is archived rather
    -- than deleted: its name is part of the explanation of that movement.
    archived_at TIMESTAMPTZ,
    created_by  TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- Defence in depth: the store validates all of this before writing, so a
    -- violation here means a bug in our code, not user input.
    CONSTRAINT inv_locations_code_shape
        CHECK (code ~ '^[A-Z0-9][A-Z0-9._-]*$' AND length(code) <= 32),
    CONSTRAINT inv_locations_name_shape CHECK (length(btrim(name)) > 0),
    CONSTRAINT inv_locations_kind_shape
        CHECK (kind IN ('stock', 'transit', 'supplier', 'customer',
                        'adjustment', 'production'))
);

-- One place per code, within the tenant and only within it: a global index
-- would leak the existence of another tenant's warehouse through a constraint
-- violation, the side channel B5.02 called out for SKUs.
CREATE UNIQUE INDEX inv_locations_code_unique ON inv_locations (tenant_id, code);

-- **At most one of each virtual counterparty per tenant.** A receipt has to be
-- able to say "from the supplier location" and mean exactly one row; two would
-- make every balance on it a half-truth. Real places (stock, transit) are
-- deliberately outside this index — a tenant may have as many warehouses and
-- vans as they own.
CREATE UNIQUE INDEX inv_locations_one_per_virtual_kind
    ON inv_locations (tenant_id, kind)
    WHERE kind IN ('supplier', 'customer', 'adjustment', 'production');

-- The list surface: a tenant's places in code order, active ones first.
CREATE INDEX inv_locations_by_code ON inv_locations (tenant_id, code);

-- Which prebuilt things a tenant has already been given — `fin_seeds`'
-- mechanism, reused whole (alo_store::inv_locations::LOCATION_SEED_KEY).
--
-- "Once" has to survive what it wrote: a tenant who deletes the warehouse we
-- gave them must not find it back the next morning, and the location rows
-- cannot answer that question once they are gone. The primary key is also what
-- makes two simultaneous first reads produce one set of locations without a
-- lock: both insert, exactly one wins, and the winner writes the rows.
CREATE TABLE inv_seeds (
    tenant_id  TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    system_key TEXT NOT NULL,
    seeded_by  TEXT NOT NULL,
    seeded_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, system_key)
);

-- The ledger. Append-only: no route and no store function updates or deletes a
-- row here. A movement recorded in error is corrected by a movement in the
-- other direction with a note — the discipline `fin_entries` holds, one layer
-- down, and for the same reason: what happened, happened, and the correction is
-- itself a fact worth keeping.
CREATE TABLE inv_moves (
    tenant_id        TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id               TEXT NOT NULL,
    -- What moved. Must be a `stocked` product — the store refuses a service,
    -- and refuses to un-stock a product that already carries movements.
    product_id       TEXT NOT NULL,
    -- Direction is the PAIR, never a sign. A signed quantity with one location
    -- column was the alternative: it makes "how much moved" a question about
    -- absolute values and reintroduces exactly the sign confusion the finance
    -- note spent a section on.
    from_location_id TEXT NOT NULL,
    to_location_id   TEXT NOT NULL,
    -- Milli-units (1.5 kg = 1500), the representation a document line already
    -- speaks (B1.06), so a purchase-order line and the movement it produces
    -- need no conversion between them. Strictly positive; bounded exactly as
    -- a line's quantity is, which keeps every sum four orders of magnitude
    -- below i64::MAX.
    qty_milli        BIGINT NOT NULL,
    -- Closed vocabulary (alo_store::inv_moves::MoveReason).
    reason           TEXT NOT NULL,
    -- What a person typed about a manual correction. Never logged (Law 1).
    note             TEXT NOT NULL DEFAULT '',
    -- The document that caused it: 'purchase_order', 'sales_order', 'count',
    -- or '' for a movement a person made directly. `ref_id` is deliberately
    -- NOT a foreign key — the kinds it points at arrive in later items, and a
    -- movement must stay readable whatever becomes of the paperwork.
    ref_kind         TEXT NOT NULL DEFAULT '',
    ref_id           TEXT NOT NULL DEFAULT '',
    -- When it physically happened, which is not when it was recorded:
    -- paperwork catches up late, and back-dating is allowed.
    occurred_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by       TEXT NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- Every leg composite and tenant-first: a movement cannot reference
    -- another tenant's product or location even if the store had a bug
    -- (docs/design/inventory.md, "Tenancy").
    --
    -- None of the three cascades. A location that has carried a movement is
    -- archived, not deleted, and the database enforces that rather than
    -- trusting the store to: the alternative silently deletes history to make
    -- a delete succeed. They restrict at the END of the statement (`NO ACTION`,
    -- the default), so dropping a whole tenant still works — `fin_postings`'
    -- arrangement, for `fin_postings`' reason.
    CONSTRAINT inv_moves_product_fk FOREIGN KEY (tenant_id, product_id)
        REFERENCES billing_products (tenant_id, id),
    CONSTRAINT inv_moves_from_fk FOREIGN KEY (tenant_id, from_location_id)
        REFERENCES inv_locations (tenant_id, id),
    CONSTRAINT inv_moves_to_fk FOREIGN KEY (tenant_id, to_location_id)
        REFERENCES inv_locations (tenant_id, id),
    CONSTRAINT inv_moves_qty_range
        CHECK (qty_milli > 0 AND qty_milli <= 1000000000),
    CONSTRAINT inv_moves_endpoints_differ
        CHECK (from_location_id <> to_location_id),
    CONSTRAINT inv_moves_reason_shape
        CHECK (reason IN ('purchase', 'sale', 'transfer', 'adjustment',
                          'count', 'return_in', 'return_out')),
    CONSTRAINT inv_moves_ref_shape
        CHECK ((ref_kind = '' AND ref_id = '')
               OR (ref_kind IN ('purchase_order', 'sales_order', 'count')
                   AND ref_id <> ''))
);

-- The ledger read: one product's history, newest first.
CREATE INDEX inv_moves_by_product
    ON inv_moves (tenant_id, product_id, occurred_at DESC, id DESC);
-- The whole tenant's history, newest first — the movement feed.
CREATE INDEX inv_moves_by_time ON inv_moves (tenant_id, occurred_at DESC, id DESC);
-- The two folds the rebuild makes, and the "what happened here" read a
-- location's drawer makes.
CREATE INDEX inv_moves_by_from ON inv_moves (tenant_id, from_location_id, product_id);
CREATE INDEX inv_moves_by_to ON inv_moves (tenant_id, to_location_id, product_id);
-- "Has this location ever carried a movement?" — the question that decides
-- whether a delete is allowed or an archive is the only removal there is. The
-- two indexes above answer it for each end.

-- The cached fold: what `inv_moves` implies, kept current so a stock screen
-- does not re-read the tenant's whole history per product per location.
--
-- Rejected: a Postgres trigger maintaining this. It would be a third language
-- in the repo for logic that belongs in the one function that already exists,
-- it is invisible to `cargo test` and to a reviewer, and it checks rows without
-- knowing intent — B4.03a's argument against a plpgsql balance trigger,
-- verbatim.
--
-- Also rejected: no cache at all, folding on read. Honest and correct, and the
-- reason not to is the reorder query (B5.07): shortages need on-hand for EVERY
-- stocked product at once, which would be a fold over the entire movement
-- history on every page load.
--
-- The row is also the LOCK that makes the negative-stock rule safe: the upsert
-- holds it until commit, so two concurrent shipments of the last unit queue
-- rather than race and exactly one of them fails. The same trade
-- `billing_sequence` made for gapless numbering, and at SME volumes it is free.
CREATE TABLE inv_stock (
    tenant_id    TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    product_id   TEXT NOT NULL,
    location_id  TEXT NOT NULL,
    -- Signed, in milli-units. Negative is legitimate and expected on the
    -- virtual counterparties — `supplier` goes ever more negative as we buy,
    -- which is the correct reading of "how much has come from outside" — and
    -- is refused on `stock` and `transit` by the store, before the row lands.
    qty_milli    BIGINT NOT NULL DEFAULT 0,
    -- The movement time last folded in. What a stock screen shows as "as of".
    last_move_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, product_id, location_id),
    -- Restricting at the end of the statement, as the ledger's keys do: a row
    -- here exists only because a movement created it, so anything that would
    -- orphan one is the same mistake.
    CONSTRAINT inv_stock_product_fk FOREIGN KEY (tenant_id, product_id)
        REFERENCES billing_products (tenant_id, id),
    CONSTRAINT inv_stock_location_fk FOREIGN KEY (tenant_id, location_id)
        REFERENCES inv_locations (tenant_id, id)
);

-- "What is at this location" — the stock screen's own read, and the one the
-- shortage query (B5.07) makes across every stocked product at once.
CREATE INDEX inv_stock_by_location ON inv_stock (tenant_id, location_id);
