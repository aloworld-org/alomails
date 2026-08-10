-- alo Inventory (ADR 0035, wave B5.06b): the bridge from a sales order into
-- alo Billing — which invoice was raised for what went out
-- (docs/design/inventory.md, "The invoice").
--
-- The decision this file exists to record is WHAT IS INVOICED, and it is a
-- ledger rather than a counter.
--
-- 1. WE BILL WHAT WAS DELIVERED, NOT WHAT WAS ORDERED. Invoicing an order
--    before it ships means asserting a VAT event on a hope. So an invoice
--    raised from an order carries each line's DELIVERED quantity, and raising a
--    second one after a second consignment must carry the new quantity ONLY.
--    That needs a per-line record of what has already been billed.
--
-- 2. THAT RECORD IS A FOLD OVER THESE ROWS, NOT AN ACCUMULATOR COLUMN — the
--    opposite of `delivered_qty_milli` (0162), and deliberately so. The
--    delivered figure had to be a column because two lines of one order may
--    name the same product, and the movement ledger could then not say which
--    line a movement belongs to. Here the link names the ORDERED LINE itself,
--    so the sum can never be ambiguous — and a fold gives two things a column
--    would have to be taught by hand:
--      * throwing away the DRAFT invoice releases the quantity, because the
--        cascade below removes these rows with it;
--      * VOIDING an issued invoice releases it too, because the fold reads
--        `billing_invoices.status` and skips a voided document.
--    An accumulator would need a release hook on both paths (the timesheet
--    handoff has exactly that, `time_invoice::release_billed_hours`), and a
--    hook is a thing that can be forgotten on the third path somebody adds.
--    A CREDIT NOTE deliberately does NOT release: crediting corrects a
--    document, the goods stay billed against the original, and re-billing them
--    would charge a customer twice for one delivery.
--
-- 3. THE LINK IS ONE-WAY AND ONE-SHOT, like every other seam into billing
--    (crm_handoff, time_invoice): inventory raises a DRAFT and never touches it
--    again. It does not issue, does not send, and holds no opinion about a
--    document a human has since edited — which is why nothing here snapshots
--    the invoice's own words. The words are the invoice's; these rows record
--    only which ordered line contributed which quantity to it.
--
-- Quantities are milli-units with the same bound every quantity in this
-- codebase carries; there is no money here at all, because the price is the
-- order line's snapshot and the totals are the invoice's own.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE inv_so_invoices (
    tenant_id  TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id         TEXT NOT NULL,
    -- The order the invoice was raised from. ON DELETE CASCADE follows the
    -- lines': only a draft order is ever deleted, and a draft has neither
    -- deliveries nor invoices.
    so_id      TEXT NOT NULL,
    -- The draft invoice this raising created. ON DELETE CASCADE is the release
    -- path: a draft somebody throws away takes its link with it, and what it
    -- carried becomes invoiceable again in the same instant.
    invoice_id TEXT NOT NULL,
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, so_id)
        REFERENCES inv_sales_orders (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, invoice_id)
        REFERENCES billing_invoices (tenant_id, id) ON DELETE CASCADE,
    -- One invoice is raised from at most one order. An invoice that carried two
    -- orders' goods would make "what has this order billed?" a question with a
    -- double-counted answer, and merging orders onto one document is a thing a
    -- human does in billing by editing lines, not a thing this seam performs.
    CONSTRAINT inv_so_invoices_one_order_per_invoice UNIQUE (tenant_id, invoice_id)
);

-- "What has been invoiced against this order", newest first.
CREATE INDEX inv_so_invoices_by_order
    ON inv_so_invoices (tenant_id, so_id, created_at DESC);

CREATE TABLE inv_so_invoice_lines (
    tenant_id     TEXT NOT NULL,
    id            TEXT NOT NULL,
    so_invoice_id TEXT NOT NULL,
    -- Which ordered line contributed. A reference, not a snapshot: the line's
    -- words, price and rate are the order's and stay there, and the invoice
    -- carries its own copy of them like every other billing line.
    so_line_id    TEXT NOT NULL,
    -- How much of that line this invoice carried, in milli-units. Positive for
    -- goods; a charge in words may be NEGATIVE, because a discount granted on
    -- the order is written as a negative quantity and is billed exactly once.
    -- Never zero: a line that contributed nothing does not reach a document.
    qty_milli     BIGINT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, so_invoice_id)
        REFERENCES inv_so_invoices (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, so_line_id)
        REFERENCES inv_sales_order_lines (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT inv_so_invoice_lines_qty_range
        CHECK (qty_milli <> 0 AND qty_milli >= -1000000000 AND qty_milli <= 1000000000),
    -- One ordered line contributes at most once per invoice: two figures for
    -- the same line on one document is a caller that has lost track of its
    -- own arithmetic.
    CONSTRAINT inv_so_invoice_lines_one_per_ordered_line
        UNIQUE (tenant_id, so_invoice_id, so_line_id)
);

-- "How much of this ordered line has already been billed" — the fold every
-- invoicing decision walks, and the one the order document reads to show what
-- is left to bill.
CREATE INDEX inv_so_invoice_lines_by_ordered_line
    ON inv_so_invoice_lines (tenant_id, so_line_id);
