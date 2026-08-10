-- alo Inventory (ADR 0035, wave B5.07): the minima a tenant keeps, per product
-- per location — the only stored input the shortage report has
-- (docs/design/inventory.md, "Reorder rules and the shortage query").
--
-- Three decisions this file records rather than assumes.
--
-- 1. **The rule is per (product, location), not per product.** "We keep ten in
--    the shop and none in the warehouse" is the normal case in a business with
--    two places, not a refinement of one. A rule per product would answer the
--    shop's question with the warehouse's stock and send somebody to the wrong
--    shelf. The unique index below makes the pair the identity of the rule.
--
-- 2. **There is no computed column here, and there never will be.** On-hand,
--    on-order and committed are all folds over rows that already exist
--    (`inv_stock`, the open purchase-order lines, the open sales-order lines),
--    and caching any of them onto this row would create exactly the drift the
--    module has refused since B5.04a: a second number that can disagree with
--    the ledger and no way to tell which one is lying. What is stored here is
--    only what a human typed.
--
-- 3. **`active` is a flag, not an archive timestamp.** The rest of the module
--    archives (a location keeps explaining the movements it carried), but a
--    reorder rule explains nothing that happened — it is a standing
--    instruction, and "stop watching this for now, without losing the numbers
--    I worked out" is exactly what a seasonal product needs. A rule nobody
--    wants back is deleted outright, which is safe here for the same reason
--    deleting a supplier's offer is (B5.03): no document copied anything from
--    it.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE inv_reorder_rules (
    tenant_id        TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id               TEXT NOT NULL,
    -- What to watch. Must be a `stocked` product — a service has no on-hand to
    -- be under, so a rule about one could never come true. CASCADE because a
    -- rule is a standing instruction about a product and means nothing without
    -- it; nothing that has *happened* is lost with it.
    product_id       TEXT NOT NULL,
    -- Where to watch it. Must be a real place (`stock`) — a minimum on the
    -- `supplier` counterparty is a minimum on a number that is negative by
    -- construction.
    location_id      TEXT NOT NULL,
    -- Milli-units, the precision every quantity in the suite speaks (B1.06):
    -- 1.5 kg = 1500. At or below the minimum is short.
    min_qty_milli    BIGINT NOT NULL,
    -- What to bring it back up to when it is short. Never below the minimum —
    -- a target under the minimum would propose a purchase that leaves the
    -- product short the moment it arrives. Equal is allowed and means "buy back
    -- to exactly the minimum".
    target_qty_milli BIGINT NOT NULL,
    -- FALSE parks the rule: it keeps its numbers and stops producing
    -- shortages. What a seasonal product needs out of season.
    active           BOOLEAN NOT NULL DEFAULT TRUE,
    created_by       TEXT NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- Both legs composite and tenant-first: a rule cannot name another
    -- tenant's product or another tenant's location even if the store had a bug
    -- (docs/design/inventory.md, "Tenancy").
    CONSTRAINT inv_reorder_rules_product_fk FOREIGN KEY (tenant_id, product_id)
        REFERENCES billing_products (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT inv_reorder_rules_location_fk FOREIGN KEY (tenant_id, location_id)
        REFERENCES inv_locations (tenant_id, id) ON DELETE CASCADE,
    -- Defence in depth: the store validates all of this before writing, so a
    -- violation here means a bug in our code, not user input. The cap is the
    -- one a PURCHASE-ORDER LINE's quantity has (`inv_purchase_order_lines`,
    -- B5.05a) rather than the larger one a supplier's minimum order may state:
    -- what this row exists to produce is a proposed order line, so a target
    -- that could not fit on one would be a number the module can compute and
    -- never act on.
    CONSTRAINT inv_reorder_rules_min_range
        CHECK (min_qty_milli >= 0 AND min_qty_milli <= 1000000000),
    CONSTRAINT inv_reorder_rules_target_range
        CHECK (target_qty_milli >= 0 AND target_qty_milli <= 1000000000),
    CONSTRAINT inv_reorder_rules_target_at_least_min
        CHECK (target_qty_milli >= min_qty_milli)
);

-- **One rule per product per place.** The pair IS the rule's identity: two
-- minima for the same shelf are two answers to one question, and a shortage
-- report that shows both is a report that has to be reconciled by hand. A
-- second write for the same pair is a `409`, not a second row.
CREATE UNIQUE INDEX inv_reorder_rules_pair_unique
    ON inv_reorder_rules (tenant_id, product_id, location_id);

-- The shortage query's own scan: every active rule this tenant holds, joined to
-- what is on the shelf. Partial, because the parked rules are never in it.
CREATE INDEX inv_reorder_rules_active
    ON inv_reorder_rules (tenant_id, location_id, product_id)
    WHERE active;

-- "What do we watch for this product?" — the product drawer's read.
CREATE INDEX inv_reorder_rules_by_product
    ON inv_reorder_rules (tenant_id, product_id);
