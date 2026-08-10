-- alo Inventory (ADR 0035, wave B5.05b): **receiving** a purchase order — the
-- goods arriving, what that does to the order, and the draft bill it raises
-- (docs/design/inventory.md, "Receiving, and the three-way-lite match").
--
-- A receipt is one delivery against one order: a location, a quantity per
-- ordered line, the movements those quantities become, and — the third leg of
-- the "three-way match, lite" — the draft bill that says what we ordered and
-- received. All of it is written in one transaction, so a tenant is never left
-- holding stock that no document explains.
--
-- Four decisions this file records rather than assumes.
--
-- 1. THE RECEIVED QUANTITY IS AN ACCUMULATOR ON THE ORDERED LINE, not a fold
--    over the movements. A movement names a product, and two lines of one
--    order may name the SAME product (two deliveries, two prices), so the
--    ledger cannot say which line a movement belongs to. The column is what
--    makes "ordered 40, received 25" a question with one answer, and the CHECK
--    below is what stops it ever exceeding what was ordered. It is written
--    only by the receiving transaction, which also writes the movements — the
--    one-writer discipline inv_stock already holds.
-- 2. THE OVER-RECEIPT BOUND IS `GREATEST(qty_milli, 0)`, deliberately not
--    written in terms of `product_id`. A free-text line (freight, a discount)
--    may carry a negative quantity and can never be received, which that
--    expression gives for free; phrasing it as "product_id IS NOT NULL AND …"
--    would re-evaluate on the ON DELETE SET NULL that deleting a catalog item
--    performs, and a received line would then block the deletion.
-- 3. A RECEIPT IS A DOCUMENT, not a bare set of movements. It has an ordinal
--    within its order (`sequence_no`), which is what the drafted bill's number
--    is built from (PO-2026-00001/R1) and what makes "the second delivery"
--    something a person can name.
-- 4. THE BILL LINK IS NULLABLE AND ON DELETE SET NULL. The bill it drafts is
--    undecided, and an undecided bill is deletable (billing_bills.rs) — a
--    mis-drafted one is thrown away and the supplier's real invoice imported
--    instead. What arrived still arrived, so the receipt outlives it.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

-- How much of this line has arrived, in the same milli-units it was ordered
-- in. 0 for every line that existed before this migration, which is exactly
-- true: nothing could be received before there was a door to receive it.
ALTER TABLE inv_purchase_order_lines
    ADD COLUMN received_qty_milli BIGINT NOT NULL DEFAULT 0;

ALTER TABLE inv_purchase_order_lines
    ADD CONSTRAINT inv_purchase_order_lines_received_range
    CHECK (received_qty_milli >= 0
           AND received_qty_milli <= GREATEST(qty_milli, 0));

CREATE TABLE inv_po_receipts (
    tenant_id     TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id            TEXT NOT NULL,
    -- The order the goods arrived against. ON DELETE CASCADE follows the
    -- lines': only a draft is ever deleted, and a draft has no receipts.
    po_id         TEXT NOT NULL,
    -- Where they were put. Any of this tenant's locations — the store holds it
    -- to a real one, because "the goods arrived at the supplier" is not a
    -- sentence.
    location_id   TEXT NOT NULL,
    -- 1 for the first delivery against this order, 2 for the next. The bill's
    -- number is built from it, and a person says "the second delivery".
    sequence_no   INTEGER NOT NULL,
    -- The day the goods arrived, from the database's own clock inside the
    -- receiving transaction — never the caller's.
    received_date DATE NOT NULL DEFAULT CURRENT_DATE,
    -- What the person who unpacked it wrote: "one crate damaged".
    note          TEXT NOT NULL DEFAULT '',
    -- The draft bill this receipt raised, while it still exists.
    bill_id       TEXT,
    created_by    TEXT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, po_id)
        REFERENCES inv_purchase_orders (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, location_id)
        REFERENCES inv_locations (tenant_id, id),
    FOREIGN KEY (tenant_id, bill_id)
        REFERENCES billing_bills (tenant_id, id) ON DELETE SET NULL (bill_id),
    CONSTRAINT inv_po_receipts_sequence_range CHECK (sequence_no >= 1),
    CONSTRAINT inv_po_receipts_note_length CHECK (length(note) <= 500)
);

-- One ordinal per order, ever: two receipts entered at the same instant cannot
-- both be "the second delivery", and neither can draft the same bill number.
CREATE UNIQUE INDEX inv_po_receipts_in_order
    ON inv_po_receipts (tenant_id, po_id, sequence_no);
-- "What has arrived against this order", newest first.
CREATE INDEX inv_po_receipts_by_order
    ON inv_po_receipts (tenant_id, po_id, received_date DESC, sequence_no DESC);

CREATE TABLE inv_po_receipt_lines (
    tenant_id  TEXT NOT NULL,
    id         TEXT NOT NULL,
    receipt_id TEXT NOT NULL,
    -- Which ordered line arrived. A reference, not a snapshot: the line's own
    -- words, price and rate are the order's and stay there.
    po_line_id TEXT NOT NULL,
    -- How much arrived now, in milli-units. Strictly positive: a receipt of
    -- nothing is not a receipt, and goods going back to a supplier are a
    -- return (a movement the other way), never a negative receipt.
    qty_milli  BIGINT NOT NULL,
    -- The movement this line wrote. NOT NULL: a receipt line with no movement
    -- would be stock that arrived from nowhere. The cascade below is not a
    -- licence to delete history — the ledger is append-only and no door
    -- removes a movement — it is what lets a whole tenant be dropped in one
    -- statement, since this table reaches `tenants` only through its receipt
    -- and would otherwise be checked before that cascade had run.
    move_id    TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, receipt_id)
        REFERENCES inv_po_receipts (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, po_line_id)
        REFERENCES inv_purchase_order_lines (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, move_id)
        REFERENCES inv_moves (tenant_id, id) ON DELETE CASCADE,
    -- The same arithmetic bound every quantity in this codebase carries.
    CONSTRAINT inv_po_receipt_lines_qty_range
        CHECK (qty_milli > 0 AND qty_milli <= 1000000000),
    -- One ordered line is booked at most once per receipt: two figures for the
    -- same line in one delivery is a caller that has lost track of its form.
    CONSTRAINT inv_po_receipt_lines_one_per_ordered_line
        UNIQUE (tenant_id, receipt_id, po_line_id)
);

-- "What has arrived against this ordered line" — the join every receipt read
-- walks, and the one the shortage report (B5.07) will.
CREATE INDEX inv_po_receipt_lines_by_ordered_line
    ON inv_po_receipt_lines (tenant_id, po_line_id);

-- A bill we drafted ourselves came from no file, so it has no syntax and no
-- checksum: both are the empty string. The two CHECKs are widened rather than
-- dropped — a bill that DOES name a syntax still has to carry a lower-case hex
-- SHA-256 of the bytes it was read from, which is the rule that matters (it is
-- what ties a stored bill to the file in the archive).
ALTER TABLE billing_bills DROP CONSTRAINT billing_bills_syntax_known;
ALTER TABLE billing_bills
    ADD CONSTRAINT billing_bills_syntax_known
    CHECK (source_syntax IN ('', 'cii', 'ubl'));
ALTER TABLE billing_bills DROP CONSTRAINT billing_bills_hashed;
ALTER TABLE billing_bills
    ADD CONSTRAINT billing_bills_hashed
    CHECK (source_sha256 ~ '^[0-9a-f]{64}$'
           OR (source_sha256 = '' AND source_syntax = ''));
