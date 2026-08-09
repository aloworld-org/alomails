-- alo Finance (ADR 0035, wave B4.09a): the confirmed match — the row that
-- turns a staged bank line into money in the books (docs/design/finance.md,
-- "The bank and reconciliation").
--
-- ONE ROW, THREE CONSEQUENCES. Confirming a match is the verb B4.08 held its
-- lines back for. It writes this row, it writes the `billing_payments` row the
-- line turned out to be, and it books both the receivable (if the invoice was
-- never posted) and the settlement — all in ONE transaction, so a tenant can
-- never hold a payment nobody booked or an entry no payment explains.
--
-- NOTHING IS EVER AUTO-CONFIRMED (ADR 0023, and here a money rule): a wrong
-- automatic match marks an invoice paid that is not, and the customer stops
-- being chased. The exact stage (`bank_match.rs`) only ever *suggests*; a
-- person confirms, and this row records who and when.
--
-- ONE LINE, ONE MATCH. The unique on (tenant_id, line_id) is the invariant the
-- line's own `status` column projects: a line is 'matched' exactly when a row
-- here names it. Splitting one bank line across several documents (a customer
-- paying three invoices in one transfer) is a later, additive change — it drops
-- this unique and keeps `amount_cents`, which is why the amount is stored per
-- match rather than read from the line.
--
-- THE TARGET IS A KIND AND AN ID, not a nullable column per document type. An
-- invoice is the only kind B4.09a confirms; a bill (B5) and an expense
-- reimbursement land as new kinds without touching the rows already here.
-- Because only 'invoice' produces a payment and an entry today, the two links
-- are nullable and a CHECK requires them exactly for that kind.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE bank_matches (
    tenant_id    TEXT NOT NULL,
    id           TEXT NOT NULL,
    -- The staged line this settles. The tenant travels in the key, so a match
    -- can never name another tenant's line, and deleting an import takes its
    -- matches with it.
    line_id      TEXT NOT NULL,
    -- What the line turned out to be: 'invoice' today; 'bill', 'expense' and
    -- 'entry' are the kinds the design names for later waves.
    target_kind  TEXT NOT NULL,
    target_id    TEXT NOT NULL,
    -- What of the line was attributed to this target, in integer cents, in the
    -- line's own currency and with the line's own sign. Equal to the line's
    -- amount for every match B4.09a writes; a column rather than a derivation
    -- because a split line is the next thing this table is asked for.
    amount_cents BIGINT NOT NULL,
    -- The payment this confirmation created, and the journal entry that
    -- settlement posted. Both are what an unmatch (B4.09c) reverses, which is
    -- why they are recorded here rather than looked up by their source key.
    payment_id   TEXT,
    entry_id     TEXT,
    -- The learned rule that proposed this match (B4.09b), when one did. NULL
    -- for a suggestion the exact stage made, which needs no rule: the payer
    -- quoted our own number.
    rule_id      TEXT,
    confirmed_by TEXT NOT NULL,
    confirmed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, line_id)
        REFERENCES bank_lines (tenant_id, id) ON DELETE CASCADE,
    -- The payment is the money fact this match asserts; removing it without
    -- removing the match would leave a line claiming to be settled by nothing.
    FOREIGN KEY (tenant_id, payment_id)
        REFERENCES billing_payments (tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, entry_id)
        REFERENCES fin_entries (tenant_id, id) ON DELETE RESTRICT,
    UNIQUE (tenant_id, line_id),
    CONSTRAINT bank_matches_target_kind
        CHECK (target_kind IN ('invoice', 'bill', 'expense', 'entry')),
    -- An invoice match is money and books; every other kind is not one yet.
    CONSTRAINT bank_matches_invoice_is_booked CHECK (
        (target_kind = 'invoice') = (payment_id IS NOT NULL AND entry_id IS NOT NULL)
    ),
    -- Zero is not a movement, and the ceiling is the typo guard every alo money
    -- column carries (±10 billion cents).
    CONSTRAINT bank_matches_amount_range
        CHECK (amount_cents <> 0 AND abs(amount_cents) <= 1000000000000),
    CONSTRAINT bank_matches_target_shape
        CHECK (char_length(target_id) BETWEEN 1 AND 64)
);

-- "Is this document already settled by a bank line?" — the read the invoice
-- screen and the unmatch path both make.
CREATE INDEX bank_matches_by_target
    ON bank_matches (tenant_id, target_kind, target_id);
