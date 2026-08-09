-- alo Finance (ADR 0035, wave B4.09b): the rules a tenant teaches the
-- reconciliation screen — "money from this counterparty is that customer's"
-- (docs/design/finance.md, "The bank and reconciliation").
--
-- A RULE PROPOSES; IT NEVER BOOKS. Like every suggestion in alo (ADR 0023, and
-- here a money rule), a rule that fires only raises a document up the ranked
-- list with the reason shown — "the rule you saved for this IBAN". A person
-- still confirms, which is what creates the payment. That is why this table has
-- no notion of confidence, no threshold and no automatic action: it is
-- somebody's own note about who their payers are.
--
-- WHAT A RULE LOOKS AT IS A COLUMN, NOT A LANGUAGE. `match_on` names one of the
-- three fields a bank states — the counterparty, the remittance, the IBAN — and
-- `pattern` is plain folded text looked for inside it (an IBAN is compared
-- whole). No globs, no regular expressions: a rule a bookkeeper cannot read back
-- is a rule they cannot trust, and a regular expression a tenant can write is a
-- denial of service they can write.
--
-- THE PATTERN IS STORED AS IT IS COMPARED — folded to lower case with its blanks
-- collapsed (`fin_match_rules.rs`) — so the unique below is the real "one rule
-- per thing to look at", not a case-sensitive imitation of it.
--
-- HITS ARE FOR THE PERSON, NOT FOR THE RANKING. The count says how often the
-- rule turned into a confirmed match, so a rules screen can show which ones earn
-- their place and which were a mistake somebody saved once. It never changes
-- what the rule scores: a heuristic that quietly re-weights itself is one nobody
-- can predict.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE fin_match_rules (
    tenant_id   TEXT NOT NULL,
    id          TEXT NOT NULL,
    -- Which field of a staged bank line this rule reads:
    -- 'counterparty', 'remittance' or 'iban'.
    match_on    TEXT NOT NULL,
    -- What to look for in it, already folded (see the header).
    pattern     TEXT NOT NULL,
    -- What the rule points at: 'invoice' today — the customer's own documents.
    -- 'bill' is the kind a supplier rule takes when B5 lands, and it arrives
    -- without touching a row already written.
    target_kind TEXT NOT NULL,
    -- Whose documents. Required for 'invoice': a rule that names no customer
    -- narrows nothing.
    customer_id TEXT,
    -- How many confirmed matches this rule proposed.
    hits        INTEGER NOT NULL DEFAULT 0,
    created_by  TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- The tenant travels in the key, so a rule can never name another tenant's
    -- customer; a customer who is deleted takes their rules with them, because a
    -- rule pointing at nobody would rank nothing for ever.
    FOREIGN KEY (tenant_id, customer_id)
        REFERENCES billing_customers (tenant_id, id) ON DELETE CASCADE,
    UNIQUE (tenant_id, match_on, pattern),
    CONSTRAINT fin_match_rules_match_on
        CHECK (match_on IN ('counterparty', 'remittance', 'iban')),
    CONSTRAINT fin_match_rules_target_kind
        CHECK (target_kind IN ('invoice', 'bill')),
    CONSTRAINT fin_match_rules_invoice_names_customer
        CHECK ((target_kind = 'invoice') = (customer_id IS NOT NULL)),
    -- Three characters is the shortest pattern that says anything; two would
    -- match half the statements in the file.
    CONSTRAINT fin_match_rules_pattern_shape
        CHECK (char_length(pattern) BETWEEN 3 AND 120),
    CONSTRAINT fin_match_rules_hits_range
        CHECK (hits >= 0)
);

-- "Which rules point at this customer?" — the read the customer screen makes,
-- and the one a merge of two customer records will need.
CREATE INDEX fin_match_rules_by_customer
    ON fin_match_rules (tenant_id, customer_id);
