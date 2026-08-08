-- alo Finance (ADR 0035, wave B4): the journal — the two tables every figure
-- this module will ever report is a fold over (docs/design/finance.md, "The
-- journal").
--
-- Four decisions carry these tables, and all four are in the note:
--
-- 1. **One signed `amount_cents`: positive is a debit, negative is a credit,
--    and the invariant is `Σ = 0` per entry.** Rejected: separate non-negative
--    `debit_cents`/`credit_cents` columns — that shape needs a CHECK that only
--    one is set, doubles every aggregate into `SUM(debit) - SUM(credit)`, and
--    restates "adds to zero" as "two sums are equal", which is the same
--    sentence written so a reader must check which column a report forgot.
--    `billing_bills` (0111) already stores a credit note in ledger direction
--    for exactly this reason. The debit/credit words survive where humans read
--    them: the journal screen renders two columns from the sign.
-- 2. **An entry is written whole, in one transaction, and never updated or
--    deleted.** There is no `updated_at` on either table and that is a signal
--    rather than an omission — the columns that would need one do not exist. A
--    correction is a *reversal*: a mirror entry carrying `reverses_entry_id`,
--    dated on or after the original. This is what makes the balance invariant
--    enforceable at all: an entry that can never be edited can only be
--    unbalanced at the instant it is written, and exactly one store function
--    (`fin_journal::post`) writes it.
-- 3. **The balance rule is enforced in Rust, in that one write path, not by a
--    `plpgsql` constraint trigger.** Rejected deliberately (the note argues it
--    at length): CLAUDE.md's two-language rule, a duplicated rule our test
--    suite does not reach the way it reaches Rust, and a trigger that sees rows
--    but not intent — it would happily pass an entry that balances and books
--    the wrong account. What the database enforces here is the shape: no
--    posting that moves nothing, no two postings in one slot, no posting to an
--    account that is not in this tenant's chart.
-- 4. **`UNIQUE (tenant_id, source_kind, source_id, source_event)` is the whole
--    idempotency mechanism.** Issuing invoice X posts ('invoice', X, 'issue');
--    a retry, a double-click or a re-run of the backfill hits this index and is
--    a typed conflict, not a second set of postings. Rejected: a `posted`
--    boolean on the document — two places to be right, and the one written
--    outside the posting transaction is the one that lies after a crash.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE fin_entries (
    tenant_id          TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id                 TEXT NOT NULL,
    -- The ACCOUNTING date: the document's own date (an invoice's issue date, a
    -- payment's value date), never the day a clerk typed. A ledger keyed on
    -- when it was entered is a ledger no period report can trust, and it is
    -- what makes the period lock (B4.10) load-bearing rather than decorative.
    entry_date         DATE NOT NULL,
    -- What kind of event this entry books. The CLOSED SET LIVES IN RUST
    -- (alo_store::fin_journal::EntryKind), like fin_accounts.role: a wave that
    -- books a new kind of document is then a code change with its own posting
    -- rule and its own tests, not a constraint swap on a tenant's books.
    kind               TEXT NOT NULL,
    -- Which document, and which of its events, produced this entry — '' for a
    -- manual entry an accountant typed, which answers to nothing but itself.
    source_kind        TEXT NOT NULL DEFAULT '',
    source_id          TEXT NOT NULL DEFAULT '',
    source_event       TEXT NOT NULL DEFAULT '',
    memo               TEXT NOT NULL DEFAULT '',
    -- The correction mechanism: this entry mirrors that one. Composite FK, so
    -- an entry can only reverse one of ITS OWN tenant's entries; NO ACTION (the
    -- default) rather than RESTRICT so that dropping a whole tenant still
    -- works — the 0106 lesson, restated below for the account link.
    reverses_entry_id  TEXT,
    -- A manual entry's evidence, as a Drive node. No FK: Drive nodes are
    -- deletable by their owner and an entry that outlives its attachment is
    -- still a true entry, whereas a RESTRICT here would make the books a reason
    -- a file cannot be tidied away.
    attachment_node_id TEXT,
    -- The currency the DOCUMENT is denominated in, and the B1.21 snapshot
    -- triple that restates it into the tenant's accounting currency. The rate
    -- is a value rather than a link: a rate row may be re-imported with a
    -- correction next week and this entry must keep saying what it was
    -- converted at (EU VAT Directive art. 91 fixes the rate at the tax point).
    currency           TEXT NOT NULL,
    fx_base_currency   TEXT NOT NULL,
    fx_rate_micro      BIGINT NOT NULL,
    fx_rate_date       DATE NOT NULL,
    -- Whose hand it was. An automatic posting carries the user whose action
    -- issued the document, which is what an audit trail means by "who".
    created_by         TEXT NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    CONSTRAINT fin_entries_reverses_fk
        FOREIGN KEY (tenant_id, reverses_entry_id) REFERENCES fin_entries (tenant_id, id),
    -- Defence in depth: the store validates all of these before writing, so a
    -- violation here means a bug in our code rather than bad user input.
    CONSTRAINT fin_entries_kind_shape
        CHECK (kind ~ '^[a-z][a-z_]{0,30}$'),
    CONSTRAINT fin_entries_source_shape
        CHECK (source_kind = '' OR source_kind ~ '^[a-z][a-z_]{0,30}$'),
    -- A source is all three parts or none of them: a source_kind with no id
    -- would take a slot in the idempotency index that no document can claim.
    CONSTRAINT fin_entries_source_whole
        CHECK ((source_kind = '' AND source_id = '' AND source_event = '')
            OR (source_kind <> '' AND source_id <> '' AND source_event <> '')),
    CONSTRAINT fin_entries_currency_shape
        CHECK (currency ~ '^[A-Z]{3}$' AND fx_base_currency ~ '^[A-Z]{3}$'),
    CONSTRAINT fin_entries_rate_positive
        CHECK (fx_rate_micro > 0),
    -- A document raised in the currency the books are kept in converts at the
    -- identity rate, always. Anything else is a rate applied to itself.
    CONSTRAINT fin_entries_identity_rate
        CHECK (currency <> fx_base_currency OR fx_rate_micro = 1000000)
);

-- One entry per document event, per tenant, forever. Partial, because a manual
-- entry has no source and any number of them may exist on one day.
CREATE UNIQUE INDEX fin_entries_source_once
    ON fin_entries (tenant_id, source_kind, source_id, source_event)
    WHERE source_kind <> '';

-- Every report this module will grow reads a date range of one tenant.
CREATE INDEX fin_entries_by_date ON fin_entries (tenant_id, entry_date, id);

-- The reversal lookup ("what corrected this?"), and the index that keeps the
-- self-referencing foreign key's check cheap.
CREATE INDEX fin_entries_by_reversed
    ON fin_entries (tenant_id, reverses_entry_id) WHERE reverses_entry_id IS NOT NULL;

CREATE TABLE fin_postings (
    tenant_id    TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id           TEXT NOT NULL,
    -- CASCADE from the entry: a posting has no life of its own, and the only
    -- delete either table will ever see is a tenant leaving.
    entry_id     TEXT NOT NULL,
    -- The order the accountant wrote the lines in — the journal screen and the
    -- CSV both read the entry back exactly as it was posted.
    position     INTEGER NOT NULL,
    account_id   TEXT NOT NULL,
    -- Signed: positive debits the account, negative credits it. `Σ = 0` over an
    -- entry, in both columns, is the invariant the write path enforces.
    amount_cents BIGINT NOT NULL,
    -- The same money in the tenant's accounting currency, crossed at the
    -- entry's own snapshot rate.
    base_cents   BIGINT NOT NULL,
    -- Which VAT rate this tax belongs to, for the postings that carry tax. The
    -- rate is a dimension here rather than an account per rate, so Germany's
    -- 19→16→19 changes no chart and no report has to know both.
    vat_rate_bp  INTEGER,
    -- The dimensions a report groups by: who owed it, whose engagement it was
    -- for, whose expense it was. Plain text rather than foreign keys — a
    -- posting must survive a customer record being tidied away, and the ledger
    -- is history, not a view of the current master data.
    customer_id  TEXT,
    supplier_key TEXT,
    project_id   TEXT,
    user_id      TEXT,
    memo         TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (tenant_id, id),
    CONSTRAINT fin_postings_entry_fk
        FOREIGN KEY (tenant_id, entry_id) REFERENCES fin_entries (tenant_id, id)
        ON DELETE CASCADE,
    -- **The link B4.02's delete guard is waiting for.** An account that carries
    -- a posting is history rather than a preference, and this is what makes
    -- `delete_fin_account` refuse against a CONCURRENT posting rather than only
    -- against a slow one (the delete then fails with SQLSTATE 23503, which the
    -- chart maps to a typed conflict).
    --
    -- NO ACTION (the default) rather than RESTRICT, which is what
    -- docs/design/finance.md wrote and what 0106 already learned the hard way:
    -- RESTRICT is checked immediately, so dropping a tenant could fail
    -- depending on which of the two cascades from `tenants` Postgres happens to
    -- run first. NO ACTION is checked at the end of the statement, by which
    -- time the postings are gone too. The guarantee for the case that matters —
    -- deleting an account that carries postings — is identical.
    CONSTRAINT fin_postings_account_fk
        FOREIGN KEY (tenant_id, account_id) REFERENCES fin_accounts (tenant_id, id),
    -- One posting per slot, so an entry read back in `position` order is the
    -- entry that was written.
    CONSTRAINT fin_postings_position_unique UNIQUE (tenant_id, entry_id, position),
    CONSTRAINT fin_postings_position_ordered CHECK (position >= 0),
    -- A posting may have `amount_cents = 0` if and only if `base_cents <> 0`:
    -- that posting is the exchange difference on a foreign-currency settlement,
    -- which moves no document money and a different number of euro. Both zero
    -- is a typo, and it is refused here as well as in the write path.
    CONSTRAINT fin_postings_moves_money
        CHECK (amount_cents <> 0 OR base_cents <> 0),
    CONSTRAINT fin_postings_vat_rate_range
        CHECK (vat_rate_bp IS NULL OR (vat_rate_bp >= 0 AND vat_rate_bp <= 10000))
);

-- The entry read: every posting of one entry, in the order it was written.
CREATE INDEX fin_postings_by_entry ON fin_postings (tenant_id, entry_id, position);

-- The report read: every posting on one account, which is what a trial
-- balance, a P&L line and the aged-receivables reconciliation all are.
CREATE INDEX fin_postings_by_account ON fin_postings (tenant_id, account_id);
