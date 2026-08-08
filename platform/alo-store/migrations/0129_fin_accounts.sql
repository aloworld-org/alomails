-- alo Finance (ADR 0035, wave B4): the chart of accounts — the list of places
-- money can be, and the ledger of which prebuilt charts a tenant has been
-- given (docs/design/finance.md, "The chart of accounts").
--
-- Two decisions carry this table, and both are in the note:
--
-- 1. **A posting rule finds its account by `role`, never by code.** A code is
--    a national convention (SKR03 and SKR04 disagree with each other before
--    either disagrees with the French PCG), so a rule that knows `1400` is a
--    rule that is wrong in every country but one, silently, in the direction
--    of a misfiled tax return. `role` is a closed set of OUR words, and at
--    most one account per tenant may hold each — enforced below by a partial
--    unique index, so "where does the receivable go" has exactly one answer
--    even when two admins are editing the chart at the same moment.
-- 2. **The default chart is seeded once per tenant, on first read**, and the
--    fact that the seed RAN is recorded in `fin_seeds` — separately from the
--    accounts it wrote, because a tenant who deletes the chart must not be
--    handed it again the next morning. This is `insight_seeds` (0121) reused
--    whole, including the primary key that makes two simultaneous first reads
--    race-free without a lock.
--
-- VAT is a dimension on the posting, not an account per rate: one `vat_output`
-- and one `vat_input` account, with the rate travelling in the posting. A rate
-- change (Germany's 19→16→19 in 2020) then changes no chart.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE fin_accounts (
    tenant_id  TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id         TEXT NOT NULL,
    -- What an accountant types and recognises. Unique within the tenant, and
    -- uppercased by the store so `ar` and `AR` can never become two accounts
    -- that look like one on a printed chart.
    code       TEXT NOT NULL,
    -- What the account is called, in the language of whoever first opened the
    -- chart. No English is written here by us: the seed's names arrive from
    -- the HTTP edge already translated (the insight_overview.rs rule).
    name       TEXT NOT NULL,
    -- The five kinds every double-entry chart has had for five centuries.
    -- This set does not grow, which is why it is a CHECK rather than a lookup
    -- table or a Rust-only rule.
    type       TEXT NOT NULL,
    -- Which of our posting-rule words this account answers to ('ar', 'bank',
    -- 'vat_output', …), or '' for an ordinary account. The CLOSED SET ITSELF
    -- lives in Rust (alo_store::fin_accounts::AccountRole) rather than in a
    -- CHECK here: a wave that needs a fourteenth role is then a code change
    -- with its own validation and tests, not a constraint swap on a table
    -- holding a tenant's books. What the database does enforce is the shape,
    -- and — below — that a role is held by at most one account.
    role       TEXT NOT NULL DEFAULT '',
    -- Inactive accounts stay readable (last year's books must still explain
    -- themselves) but drop out of the pickers and refuse new postings.
    active     BOOLEAN NOT NULL DEFAULT TRUE,
    -- Seeded by us. A system account is renameable and recodeable — a tenant
    -- whose accountant wants `1400` for receivables must be able to say so —
    -- but never deletable: the posting rules resolve through it.
    system     BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- The accountant's own key. Also the seed's idempotency: re-running it
    -- can only ever collide, never duplicate.
    CONSTRAINT fin_accounts_code_unique UNIQUE (tenant_id, code),
    -- Defence in depth: the store validates all of these before writing, so a
    -- violation here means a bug in our code rather than bad user input.
    CONSTRAINT fin_accounts_type_known
        CHECK (type IN ('asset', 'liability', 'equity', 'income', 'expense')),
    CONSTRAINT fin_accounts_code_shape
        CHECK (code <> '' AND char_length(code) <= 20 AND code !~ '\s'),
    CONSTRAINT fin_accounts_name_shape
        CHECK (name <> '' AND char_length(name) <= 120),
    CONSTRAINT fin_accounts_role_shape
        CHECK (role = '' OR role ~ '^[a-z][a-z_]{0,30}$')
);

-- At most one account per role per tenant — the index that makes the by-role
-- lookup a single row by construction rather than by hope. Partial, because ''
-- is not a role: an ordinary chart has dozens of accounts holding none.
CREATE UNIQUE INDEX fin_accounts_one_per_role
    ON fin_accounts (tenant_id, role) WHERE role <> '';

-- The read the posting rules make on every document: "the account for this
-- role, in this tenant". Covered by the unique index above; stated here only
-- as the intent it serves.

CREATE TABLE fin_seeds (
    tenant_id   TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    -- Which prebuilt thing: 'eu_sme_chart' is the only key B4.02 mints. The
    -- backfill that opens a tenant's books over documents older than its
    -- ledger (docs/design/finance.md, "When the books open") takes its own key
    -- here, for the same reason and with the same guarantee.
    system_key  TEXT NOT NULL,
    -- Whoever opened the chart first. The accounts the seed wrote carry no
    -- author column at all; this row is the only place that fact lives.
    seeded_by   TEXT NOT NULL,
    seeded_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, system_key)
);
