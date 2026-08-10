-- alo Finance (ADR 0035, wave B4.14a): the category a machine THINKS a claim
-- belongs to, kept apart from the one a person agreed
-- (docs/design/finance.md, "The finance agent").
--
-- The agent's `categorise_transactions` proposes a category for claims nobody
-- has classified. A proposal is not a classification, and the difference is a
-- column rather than a convention: `category_id` stays exactly what it was
-- before this migration — the word a HUMAN chose, and the only thing any
-- posting rule, report or VAT return ever reads — while `proposed_category_id`
-- is what was suggested and nothing else looks at. Writing a suggestion into
-- `category_id` would put a guess in the P&L, and nobody downstream could tell
-- it from a decision (ADR 0023: the agent proposes, the person approves).
--
-- Accepting is therefore a *move* between the two columns, and dismissing is
-- clearing one of them. Both are the claimant's own verbs on their own claim.
--
-- Expand-only, as every migration on this track is: three nullable columns on
-- an existing table, no data rewritten, no column dropped. A build that has not
-- yet seen this migration reads and writes claims exactly as before.

ALTER TABLE fin_expenses
    -- What the agent suggests this claim books to. NULL is the normal state:
    -- most claims either carry a category already or have never been asked
    -- about. Composite foreign key, so the suggestion is always the same
    -- tenant's own word; NO ACTION rather than RESTRICT for 0106's lesson (a
    -- tenant delete must not depend on which cascade Postgres runs first).
    ADD COLUMN proposed_category_id TEXT,
    -- When it was suggested. Its presence is what "there is a proposal" means,
    -- and its age is what a person judges a stale suggestion by.
    ADD COLUMN proposed_at TIMESTAMPTZ,
    -- WHY it was suggested, as a machine-readable code ('merchantHistory'
    -- today) — never a sentence. A sentence stored here would be a user-facing
    -- string authored by the server in one language, which is a bug in a
    -- European product; the client writes the words in the reader's own.
    ADD COLUMN proposed_reason TEXT NOT NULL DEFAULT '',
    -- When the claimant said no. Kept after the proposal itself is cleared,
    -- because "no" has to survive: without it, the next run of the tool would
    -- offer the same rejected word again, and a suggestion a person has to
    -- refuse twice is one they stop reading. A human can still classify the
    -- claim by hand — this only silences the machine.
    ADD COLUMN proposal_declined_at TIMESTAMPTZ;

ALTER TABLE fin_expenses
    -- Half a proposal describes nothing: a category with no day it was
    -- suggested, or a day with nothing suggested, would both print as an offer
    -- the screen cannot make. Same shape as `fin_expenses_decision_whole`.
    ADD CONSTRAINT fin_expenses_proposal_whole
        CHECK (num_nonnulls(proposed_category_id, proposed_at) <> 1),
    -- A reason with no proposal is a leftover from a dismissal that only half
    -- happened; clearing a proposal clears all three columns.
    ADD CONSTRAINT fin_expenses_proposal_reason_shape
        CHECK ((proposed_category_id IS NULL AND proposed_reason = '')
               OR (proposed_category_id IS NOT NULL
                   AND proposed_reason <> '' AND char_length(proposed_reason) <= 40)),
    ADD CONSTRAINT fin_expenses_proposed_category_fk
        FOREIGN KEY (tenant_id, proposed_category_id)
        REFERENCES fin_categories (tenant_id, id);

-- "What is waiting for me to agree to it?" — the claimant's own read, and the
-- only one that exists. Partial, because a proposal is the exception: almost
-- every row in this table has none.
CREATE INDEX fin_expenses_with_a_proposal
    ON fin_expenses (tenant_id, user_id, spent_on DESC)
    WHERE proposed_category_id IS NOT NULL;
