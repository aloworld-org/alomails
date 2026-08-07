-- alo Billing (ADR 0035, wave B2.12): the mark a bill carries once it has been
-- put into a SEPA credit-transfer file (pain.001) for the bank to execute.
--
-- Why this is stored at all, when the file itself is a download: **paying a
-- supplier twice is the accident this whole record type exists to prevent.**
-- `billing_bills` already refuses the same supplier document being imported
-- twice (0111); the mirror of that rule on the way out is that the same bill is
-- not silently handed to the bank twice. A bookkeeper who exports on Monday and
-- again on Tuesday, having forgotten, is told the bill is already in a file and
-- when — and may still repeat it deliberately, which is a different act (the
-- bank rejected the file, the file was lost) and reads as one.
--
-- Expand-only, all three columns nullable with no backfill: every bill in
-- existence today has never been exported, and NULL says exactly that. No
-- default, because "never exported" is the absence of an event rather than a
-- value.
--
-- The message id is the `MsgId` of the file the bill went into (ISO 20022
-- `GrpHdr/MsgId`, at most 35 characters of the SEPA-restricted character set).
-- It is what ties a row to a payment run a bank quotes back at you, so it is
-- kept on the bill rather than only in a file nobody stored: the file is a
-- download, and the tenant's copy of it is their bank's problem, but which run
-- a liability was paid in is ours.
--
-- Deliberately NOT a `paid` status. A file handed to a bank is an instruction,
-- not a settlement: the money moves when the bank says it moved, which arrives
-- back as a bank statement line and is reconciled in B4.09. Calling this
-- "paid" would book a payment that may still be refused.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

ALTER TABLE billing_bills
    ADD COLUMN exported_at        TIMESTAMPTZ,
    ADD COLUMN exported_by        TEXT,
    ADD COLUMN export_message_id  TEXT,
    -- The three are one fact and can never be half-written: a bill is either
    -- in a payment file — with the moment, the person and the run all named —
    -- or it is not.
    ADD CONSTRAINT billing_bills_export_complete
        CHECK ((exported_at IS NULL) = (exported_by IS NULL)
               AND (exported_at IS NULL) = (export_message_id IS NULL)),
    -- Defence in depth against a writer that has not restricted the message id
    -- to what a SEPA message may carry (EPC character set, Max35Text): a file a
    -- bank refuses to parse is worse than a refusal here.
    ADD CONSTRAINT billing_bills_export_message_id_shape
        CHECK (export_message_id IS NULL
               OR (length(export_message_id) BETWEEN 1 AND 35
                   AND export_message_id ~ '^[A-Za-z0-9/?:().,''+ -]+$'));

-- The payment run's own read: what this tenant has approved and not yet sent
-- to the bank, oldest liability first — the order a payment run is prepared in,
-- because the oldest bill is the one closest to being late.
CREATE INDEX billing_bills_payable
    ON billing_bills (tenant_id, due_date, issue_date, id)
    WHERE status = 'approved' AND exported_at IS NULL;
