-- alo Billing (ADR 0035, wave B2.11): recurring invoices — the standing
-- arrangement that raises the same invoice again every month, quarter or year.
--
-- A SCHEDULE NEVER ISSUES ANYTHING. What a due run produces is a DRAFT, which
-- a human then reads and issues by hand. That is not timidity: issuing spends a
-- number out of a legally gapless series (migration 0103) and freezes a
-- document a customer and a tax authority may act on, and no unattended job of
-- ours is going to do that on a tenant's behalf. It is also what
-- `docs/features.md` [B2] asks for in as many words — "auto-draft for
-- approval".
--
-- The template is a SNAPSHOT, exactly like a document's lines are (0102): the
-- schedule carries its own copy of the customer, currency, terms and lines, so
-- editing next month's price list — or the arrangement itself — never rewrites
-- the drafts already raised from it.
--
-- `next_run_date` is stored rather than derived on every read. It is the one
-- column a run takes a lock on and moves, which is what makes "raise the
-- invoice for March" happen exactly once however many runs race: the row lock
-- serialises them, and the unique index at the bottom of this file is the
-- database's own last word on the same rule.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE billing_schedules (
    tenant_id          TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id                 TEXT NOT NULL,
    -- The party billed, pinned to the SAME tenant by the composite reference
    -- below, so even a bug in a WHERE clause cannot bill across tenants.
    customer_id        TEXT NOT NULL,
    -- What a human calls this arrangement ("Hosting — Acme, monthly"). Shown in
    -- the list; never printed on the invoice it raises.
    name               TEXT NOT NULL,
    -- 'weekly' | 'monthly' | 'quarterly' | 'yearly' (crate::billing_cadence).
    cadence            TEXT NOT NULL,
    -- The day of the month the arrangement is anchored to, taken from
    -- `start_date` and then never moved: a schedule anchored to the 31st bills
    -- on the 28th in February and on the 31st again in March. Ignored by the
    -- weekly cadence, which keeps its weekday instead.
    anchor_day         SMALLINT NOT NULL,
    -- The first date it bills on, kept for the record (the anchor and the audit
    -- trail both come from it) even after `next_run_date` has moved past it.
    start_date         DATE NOT NULL,
    -- The last date it may bill on, or NULL for "until somebody stops it".
    end_date           DATE,
    -- The next date a run will raise a draft for. Moves forward one occurrence
    -- per draft raised, under the row's lock.
    next_run_date      DATE NOT NULL,
    -- When a run last raised anything for this schedule. NULL until the first.
    last_run_date      DATE,
    -- Paused arrangements stay in the list with their dates intact; a paused
    -- schedule is skipped by every run and resumes where it left off.
    active             BOOLEAN NOT NULL DEFAULT true,
    -- Snapshotted from the source invoice, exactly as a document snapshots them
    -- (0102): the arrangement bills in the currency and on the terms it was set
    -- up with, whatever the customer's record says next year.
    currency           TEXT NOT NULL DEFAULT 'EUR',
    payment_terms_days INTEGER NOT NULL DEFAULT 30,
    -- Copied onto every draft this schedule raises.
    reference          TEXT NOT NULL DEFAULT '',
    note               TEXT NOT NULL DEFAULT '',
    -- Who set it up. The drafts a background run raises are created as this
    -- user, because it is their standing instruction that raised them.
    created_by         TEXT NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, customer_id)
        REFERENCES billing_customers (tenant_id, id) ON DELETE CASCADE,
    -- Defence in depth: the store validates all of this before writing, so a
    -- violation here means a bug in our code, not bad user input.
    CONSTRAINT billing_schedules_cadence_known
        CHECK (cadence IN ('weekly', 'monthly', 'quarterly', 'yearly')),
    CONSTRAINT billing_schedules_anchor_day_range
        CHECK (anchor_day >= 1 AND anchor_day <= 31),
    CONSTRAINT billing_schedules_name_shape CHECK (length(btrim(name)) > 0),
    -- An arrangement that ends before it starts bills nothing, which is a
    -- mistake to report rather than a state to store.
    CONSTRAINT billing_schedules_ends_after_it_starts
        CHECK (end_date IS NULL OR end_date >= start_date),
    -- A run only ever moves this forward, never behind the first occurrence.
    CONSTRAINT billing_schedules_next_run_not_before_start
        CHECK (next_run_date >= start_date),
    CONSTRAINT billing_schedules_currency_shape
        CHECK (currency ~ '^[A-Z]{3}$'),
    CONSTRAINT billing_schedules_terms_range
        CHECK (payment_terms_days >= 0 AND payment_terms_days <= 365)
);

-- The sweep's read: every active schedule of every tenant that is due today.
-- Deliberately NOT tenant-first — the background runner asks the question
-- across tenants, and then does the work through each tenant's own account
-- door (crate::billing_schedules).
CREATE INDEX billing_schedules_due
    ON billing_schedules (next_run_date)
    WHERE active;
-- The module's own list, and "what does this customer pay us every month".
CREATE INDEX billing_schedules_by_customer
    ON billing_schedules (tenant_id, customer_id, created_at DESC);

-- The template lines, the same model as an invoice's (crate::billing_line):
-- copying the schedule onto a draft is then a copy, not a translation.
CREATE TABLE billing_schedule_lines (
    tenant_id        TEXT NOT NULL,
    schedule_id      TEXT NOT NULL,
    id               TEXT NOT NULL,
    line_order       INTEGER NOT NULL,
    description      TEXT NOT NULL,
    unit             TEXT NOT NULL DEFAULT '',
    qty_milli        BIGINT NOT NULL,
    unit_price_cents BIGINT NOT NULL,
    vat_rate_bp      INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, id),
    -- No tenants(id) reference of its own: a line reaches its tenant only
    -- through its schedule, which is the single place that link is stated.
    FOREIGN KEY (tenant_id, schedule_id)
        REFERENCES billing_schedules (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT billing_schedule_lines_description_shape
        CHECK (length(btrim(description)) > 0),
    CONSTRAINT billing_schedule_lines_order_range CHECK (line_order >= 0),
    -- The same bounds as an invoice line, for the same reason: they are what
    -- keep the totals arithmetic inside i64 (billing_line.rs).
    CONSTRAINT billing_schedule_lines_qty_range
        CHECK (qty_milli >= -1000000000 AND qty_milli <= 1000000000),
    CONSTRAINT billing_schedule_lines_price_range
        CHECK (unit_price_cents >= 0 AND unit_price_cents <= 1000000000),
    CONSTRAINT billing_schedule_lines_vat_rate_range
        CHECK (vat_rate_bp >= 0 AND vat_rate_bp <= 10000)
);

CREATE UNIQUE INDEX billing_schedule_lines_in_order
    ON billing_schedule_lines (tenant_id, schedule_id, line_order);

-- Where a draft came from, added to the document itself: the arrangement that
-- raised it, and WHICH occurrence of that arrangement it is for. Both NULL on
-- every invoice raised by a human, which is all of them until now — an
-- expand-only column with no default to backfill.
--
-- Deliberately NOT ON DELETE CASCADE and not SET NULL: a schedule that has
-- raised documents cannot be deleted at all (it is paused instead), because
-- deleting it would either take real invoices with it or quietly erase where
-- they came from. The store says so in words; this is the database agreeing.
ALTER TABLE billing_invoices
    ADD COLUMN schedule_id       TEXT,
    ADD COLUMN schedule_due_date DATE,
    ADD CONSTRAINT billing_invoices_schedule_fk
        FOREIGN KEY (tenant_id, schedule_id)
        REFERENCES billing_schedules (tenant_id, id),
    ADD CONSTRAINT billing_invoices_schedule_names_its_occurrence
        CHECK ((schedule_id IS NULL) = (schedule_due_date IS NULL));

-- One draft per occurrence, ever. The run already serialises on the schedule's
-- row lock; this is the database's independent guarantee that no arrangement
-- can bill the same period twice, whatever a future caller does. Postgres
-- allows many NULLs in a unique index, so hand-raised invoices never collide.
CREATE UNIQUE INDEX billing_invoices_one_per_occurrence
    ON billing_invoices (tenant_id, schedule_id, schedule_due_date);
