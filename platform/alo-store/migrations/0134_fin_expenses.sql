-- alo Finance (ADR 0035, wave B4): what a person spent, and the word that says
-- where it books (docs/design/finance.md, "Expenses, receipts and mileage").
--
-- Two tables and one sentence each:
--
-- `fin_categories` is tenant-wide configuration — the handful of words a claim
-- form offers ("Travel", "Software", "Meals") and, for each, the account of the
-- chart (0129) it books to. The category is the ONLY thing that decides an
-- expense's account, which is why it is a row rather than a string on the
-- claim: renaming "Software" or repointing it at a different account must not
-- rewrite what a hundred old claims booked to, and must not require re-typing
-- the mapping on every future one.
--
-- `fin_expenses` is one claim. AN EXPENSE CLAIM IS PERSONAL DATA ABOUT AN
-- EMPLOYEE — a receipt names a restaurant, a pharmacy, a city on a date — so
-- `user_id` is not a convenience column but the key the account door binds on
-- every statement, exactly as `time_entries` (0123) does for hours. A
-- colleague's claim is unrepresentable on that door, not merely refused; the
-- approver's cross-user read is a tenant-door query behind a role gate and
-- arrives with the approval flow (B4.05b). Merchant, description and decision
-- note never reach a log.
--
-- VAT IS STATED, NEVER DERIVED. A receipt showing €119 with 19 % is entered as
-- gross 119 / VAT 19; a receipt showing only a total is entered with VAT 0 and
-- books nothing to `vat_input`. Reclaiming input VAT a receipt does not
-- evidence is a false statement on a return, and the difference between "the
-- receipt does not show it" and "the receipt shows zero" is exactly what a tax
-- inspector asks about. Hence: two integer-cent columns and a nullable rate,
-- and never a computation from a category default.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE fin_categories (
    tenant_id           TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id                  TEXT NOT NULL,
    -- The word on the claim form, in the tenant's own language. No English is
    -- written here by us: this table ships empty and a tenant names their own
    -- categories (the `insight_overview.rs` rule — nothing we seed carries a
    -- hardcoded English string).
    name                TEXT NOT NULL,
    -- Where a claim in this category books. Composite so the account is always
    -- the same tenant's, and NO ACTION (the default) rather than RESTRICT for
    -- 0106's lesson: RESTRICT is checked immediately, so dropping a whole
    -- tenant could fail depending on which cascade from `tenants` Postgres runs
    -- first. NO ACTION is checked at the end of the statement, by which time
    -- the categories are gone too. What it still enforces — deleting an account
    -- a category points at fails with 23503 — is the rule it exists for.
    account_id          TEXT NOT NULL,
    -- The rate the claim form OFFERS for this category, or NULL for "the
    -- receipt says". It is a default in the UI sense only: nothing derives an
    -- expense's VAT from it (see the header).
    default_vat_rate_bp INTEGER,
    -- An inactive category stays readable — last year's claims must still
    -- explain themselves — and drops out of the picker.
    active              BOOLEAN NOT NULL DEFAULT TRUE,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    CONSTRAINT fin_categories_account_fk FOREIGN KEY (tenant_id, account_id)
        REFERENCES fin_accounts (tenant_id, id),
    -- Defence in depth: the store validates each of these before writing, so a
    -- violation here means a bug in our code rather than bad user input.
    CONSTRAINT fin_categories_name_shape
        CHECK (name <> '' AND char_length(name) <= 120),
    CONSTRAINT fin_categories_vat_rate_range
        CHECK (default_vat_rate_bp IS NULL
               OR (default_vat_rate_bp >= 0 AND default_vat_rate_bp <= 10000))
);

-- Two categories a person cannot tell apart are a picker with a coin toss in
-- it, and a claim booked by the losing one. Case-insensitive, because "Travel"
-- and "travel" are one word to everybody reading the form.
CREATE UNIQUE INDEX fin_categories_name_unique
    ON fin_categories (tenant_id, lower(name));

CREATE TABLE fin_expenses (
    tenant_id       TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id              TEXT NOT NULL,
    -- Whose claim. Bound from the account door on every statement, never taken
    -- from request input.
    user_id         TEXT NOT NULL,
    -- The day the money left, in the claimant's own zone — a purchase is a
    -- calendar fact, and it is what every period boundary uses (the VAT return
    -- included). Never an instant, for `time_entries`' reason.
    spent_on        DATE NOT NULL,
    -- The word that decides the account. NULL is legitimate and books to the
    -- chart's `expense_default` role: a claim whose category nobody has agreed
    -- yet is still a claim, and refusing it would lose the receipt to protect
    -- the classification. NO ACTION for the reason above; a category in use
    -- cannot be deleted (23503), which is the rule.
    category_id     TEXT,
    -- Who was paid, and what for. Both personal data: never logged.
    merchant        TEXT NOT NULL DEFAULT '',
    description     TEXT NOT NULL DEFAULT '',
    -- What the receipt says, in integer cents, and never a float. `vat_cents`
    -- is the amount the receipt SHOWS (0 when it shows none), and
    -- `vat_rate_bp` the rate beside it — see the header. Net is
    -- `gross_cents - vat_cents`, computed where it is displayed rather than
    -- stored, so the two columns can never disagree with a third.
    gross_cents     BIGINT NOT NULL,
    vat_cents       BIGINT NOT NULL DEFAULT 0,
    vat_rate_bp     INTEGER,
    currency        TEXT NOT NULL,
    -- Whose pocket it came out of, which decides what the approval books:
    -- `personal` credits the employee (they are owed it), `card` and `cash`
    -- credit the company's own money (nobody is owed anything).
    method          TEXT NOT NULL,
    -- The engagement this cost belongs to (the B3 bridge and the rebill hook).
    -- Deliberately carries NO foreign key: deleting a board must not delete
    -- money a person is owed, and the composite `ON DELETE SET NULL` that would
    -- express "forget the link, keep the claim" would null `tenant_id` too. A
    -- dangling id simply resolves to nothing, exactly as `time_entries.task_id`
    -- does.
    project_id      TEXT,
    -- The receipt in Drive. No foreign key for the same reason, and one more:
    -- purging a file must not delete the claim it evidenced. The store checks
    -- the caller can read the node when it is set.
    receipt_node_id TEXT,
    -- Where the claim is in the flow. The transitions and who may make them are
    -- B4.05b's; what this migration fixes is the vocabulary and the two facts a
    -- status implies (below).
    status          TEXT NOT NULL DEFAULT 'draft',
    submitted_at    TIMESTAMPTZ,
    decided_by      TEXT,
    decided_at      TIMESTAMPTZ,
    decision_note   TEXT NOT NULL DEFAULT '',
    reimbursed_on   DATE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    CONSTRAINT fin_expenses_category_fk FOREIGN KEY (tenant_id, category_id)
        REFERENCES fin_categories (tenant_id, id),
    -- Defence in depth: the store validates every one of these before writing.
    -- A claim of nothing is not a claim; the ceiling is `UNIT_PRICE_MAX_CENTS`,
    -- the same typo guard every other alo money field carries.
    CONSTRAINT fin_expenses_gross_range
        CHECK (gross_cents >= 1 AND gross_cents <= 1000000000),
    -- VAT is part of the gross, so it cannot exceed it — the one arithmetic
    -- claim a receipt cannot make.
    CONSTRAINT fin_expenses_vat_within_gross
        CHECK (vat_cents >= 0 AND vat_cents <= gross_cents),
    CONSTRAINT fin_expenses_vat_rate_range
        CHECK (vat_rate_bp IS NULL OR (vat_rate_bp >= 0 AND vat_rate_bp <= 10000)),
    -- A VAT amount needs the rate it was charged at: a return line is a rate
    -- and a figure, and a figure with no rate cannot go on one.
    CONSTRAINT fin_expenses_vat_amount_has_rate
        CHECK (vat_cents = 0 OR (vat_rate_bp IS NOT NULL AND vat_rate_bp > 0)),
    CONSTRAINT fin_expenses_currency_shape CHECK (currency ~ '^[A-Z]{3}$'),
    CONSTRAINT fin_expenses_method_known
        CHECK (method IN ('personal', 'card', 'cash')),
    CONSTRAINT fin_expenses_status_known
        CHECK (status IN ('draft', 'submitted', 'approved', 'rejected', 'reimbursed')),
    CONSTRAINT fin_expenses_merchant_shape CHECK (char_length(merchant) <= 120),
    CONSTRAINT fin_expenses_description_shape CHECK (char_length(description) <= 500),
    CONSTRAINT fin_expenses_note_shape CHECK (char_length(decision_note) <= 500),
    -- A decision is one fact: who made it and when. Half of it describes
    -- nothing and would print as "decided by nobody".
    CONSTRAINT fin_expenses_decision_whole
        CHECK (num_nonnulls(decided_by, decided_at) <> 1),
    -- Anything past draft has been handed in, and a reimbursed claim has a day
    -- the money moved — the date the reimbursement books on.
    CONSTRAINT fin_expenses_submitted_when_past_draft
        CHECK (status = 'draft' OR submitted_at IS NOT NULL),
    CONSTRAINT fin_expenses_reimbursed_has_day
        CHECK (status <> 'reimbursed' OR reimbursed_on IS NOT NULL)
);

-- "My claims", newest first — the personal door's list, and the only read that
-- exists before B4.05b.
CREATE INDEX fin_expenses_by_user_date
    ON fin_expenses (tenant_id, user_id, spent_on DESC);
-- The approver's inbox (B4.05b): everything awaiting a decision, in the order a
-- queue is worked.
CREATE INDEX fin_expenses_by_status
    ON fin_expenses (tenant_id, status, spent_on);
-- "What has this engagement cost?" — the project report and the rebill hook.
-- Partial, because most claims belong to no project.
CREATE INDEX fin_expenses_by_project
    ON fin_expenses (tenant_id, project_id) WHERE project_id IS NOT NULL;
