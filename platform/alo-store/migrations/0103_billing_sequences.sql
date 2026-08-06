-- alo Billing (ADR 0035, wave B1): the per-tenant document-number counters
-- behind legally gapless numbering.
--
-- WHY A TABLE AND NOT A POSTGRES SEQUENCE (docs/design/billing.md): a
-- `SEQUENCE` is deliberately non-transactional, so a rolled-back or failed
-- transaction BURNS the number it drew and leaves a permanent hole. Gapless
-- numbering is a legal requirement for invoices across the EU (§14 UStG in DE
-- and the equivalents in FR/BE/NL), so the very property that makes
-- `nextval()` contention-free is the property that makes it unusable here.
-- This row is drawn inside the same transaction that writes the number onto
-- the invoice: if that transaction rolls back, the counter rolls back with it.
--
-- One row per (tenant, kind, year): the counter resets each year, which is
-- what the `INV-YYYY-NNNNN` format states, and each tenant counts alone.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE billing_sequences (
    tenant_id  TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    -- Which series this counts. 'invoice' today — credit notes draw from the
    -- SAME series so the ledger stays continuous (docs/design/billing.md);
    -- quotes (B1.11) get their own kind. Shape-checked rather than
    -- list-checked, so adding a series is a new row, never a schema change.
    kind       TEXT NOT NULL,
    -- The calendar year of the issue date the number was drawn for.
    year       INTEGER NOT NULL,
    -- The number the NEXT document of this series will carry. A row that has
    -- never been drawn from does not exist; the first draw creates it at 2 and
    -- takes 1.
    next_value BIGINT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, kind, year),
    CONSTRAINT billing_sequences_kind_shape CHECK (kind ~ '^[a-z_]{1,32}$'),
    CONSTRAINT billing_sequences_year_range CHECK (year >= 2000 AND year <= 9999),
    -- The first number handed out is 1, so the next one is always at least 2
    -- once the row exists.
    CONSTRAINT billing_sequences_value_range CHECK (next_value >= 2)
);
