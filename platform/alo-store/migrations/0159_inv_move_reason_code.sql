-- alo Inventory (ADR 0035, wave B5.04b): the reason code a manual adjustment
-- carries (docs/design/inventory.md, "Adjustments and transfers").
--
-- `reason` already says a movement was an adjustment rather than a purchase or
-- a delivery. It does not say WHY the shelf disagreed with the system, and
-- "why is stock disappearing" is the one question a stock adjustment exists to
-- answer. A free-text field answers it with the empty string, so the answer is
-- a closed vocabulary (alo_store::inv_adjust::AdjustReason): damaged, lost,
-- expired, found, sample, internal_use, correction.
--
-- The column is expand-only: `NOT NULL DEFAULT ''` writes no row and reads
-- every existing movement as "no code", which is what a purchase, a delivery
-- or a transfer means. The rule that a code is present exactly when the reason
-- is `adjustment` is enforced by `alo_store::inv_moves`, and by the constraint
-- below for anything that ever writes past it.

ALTER TABLE inv_moves ADD COLUMN reason_code TEXT NOT NULL DEFAULT '';

-- A code belongs to an adjustment and to nothing else: a receipt is explained
-- by its purchase order, a delivery by its sales order, a transfer by the two
-- places it names. `count` is deliberately outside this rule too — a stocktake
-- variance is explained by the count sheet it came from (B5.08b).
--
-- **NOT VALID on purpose.** The check applies to every row written from here
-- on, and does not re-read the ledger — which is append-only, so "from here on"
-- is every row that can still be wrong. Validating history instead would make
-- this migration fail on any database that already carries adjustment
-- movements from before the vocabulary existed (every developer's, after
-- B5.04a's property suite ran), and rewriting those rows to please a
-- constraint is exactly the destructive DDL the ledger's whole design refuses.
ALTER TABLE inv_moves ADD CONSTRAINT inv_moves_reason_code_shape
    CHECK ((reason = 'adjustment') = (reason_code <> '')
           AND (reason_code = '' OR reason_code IN ('damaged', 'lost', 'expired',
                                                    'found', 'sample',
                                                    'internal_use', 'correction')))
    NOT VALID;
