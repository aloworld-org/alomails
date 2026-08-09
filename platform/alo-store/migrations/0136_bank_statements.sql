-- alo Finance (ADR 0035, wave B4.08): the bank statement, and the lines it
-- stages for reconciliation (docs/design/finance.md, "The bank and
-- reconciliation").
--
-- A STAGED LINE IS NOT AN EVENT. Everything else in the ledger is booked the
-- moment it becomes real; a bank line is deliberately the opposite. It is what
-- the bank says happened, held apart from the books until a human confirms what
-- it *was* — which invoice it paid, which expense it settled, or that it is not
-- ours to book at all. Nothing here posts. Nothing here is auto-matched (ADR
-- 0023, and here it is also a money rule: a wrong automatic match marks an
-- invoice paid that is not, and the customer stops being chased). Confirming a
-- match is B4.09's verb, and it is what creates the payment and its postings.
--
-- A FILE IMPORTS ONCE AND A LINE IMPORTS ONCE. Bookkeepers re-upload; banks
-- publish overlapping files; a month's statement arrives again inside the
-- quarter's. So two uniqueness rules carry the whole of the duplicate story:
-- `file_sha256` per tenant (the same bytes are the same import, refused as a
-- conflict naming what is already there) and `line_hash` per tenant (a line
-- already staged from another file is skipped, and the import report says how
-- many and why). REJECTED: trusting the bank's own reference alone — some banks
-- reuse it across statements, some omit it entirely, and neither failure is
-- visible until money is booked twice.
--
-- The balances are NULLABLE, which the rest of alo's money columns are not. A
-- balance the bank did not state is absent, not zero, and a zero here would be
-- a reconciliation target that quietly disagrees with reality. Refusing such a
-- file instead would throw away every line in it over a figure that is a check,
-- not the point of the import.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE bank_statements (
    tenant_id             TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id                    TEXT NOT NULL,
    -- The account the statement is *of*, as the file states it. Kept as text
    -- rather than joined to anything: alo does not model the tenant's bank
    -- accounts yet, and the IBAN is what every one of the three formats agrees
    -- on. Uppercased, no spaces, shape-checked by the store.
    account_iban          TEXT NOT NULL,
    currency              TEXT NOT NULL,
    -- Which parser read the file. Kept because the three disagree about what
    -- they can tell us — an MT940 line has no IBAN, a CSV has whatever the
    -- mapping said — and a reader of a staged line needs to know which
    -- silence it is looking at.
    source                TEXT NOT NULL,
    -- The bank's own name for this statement (`<Stmt><Id>` in CAMT, `:28C:` in
    -- MT940): the number a person cross-checks against the paper. Optional,
    -- because a CSV export has none.
    statement_ref         TEXT NOT NULL DEFAULT '',
    -- SHA-256 of the file exactly as uploaded, lowercase hex. The same bytes
    -- are the same import.
    file_sha256           TEXT NOT NULL,
    -- What the bank said the account held at the start and the end of the
    -- period. NULL means the file stated no such balance (see the header).
    opening_balance_cents BIGINT,
    closing_balance_cents BIGINT,
    from_date             DATE NOT NULL,
    to_date               DATE NOT NULL,
    imported_by           TEXT NOT NULL,
    imported_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- The file-level duplicate rule. Per tenant, never global: two tenants
    -- banking with the same bank can legitimately hold byte-identical files
    -- (an empty month at the same institution), and a global unique would make
    -- one tenant's import an oracle for another's.
    UNIQUE (tenant_id, file_sha256),
    -- Defence in depth: the store validates each of these before writing, so a
    -- violation here means a bug in our code rather than a bad file.
    CONSTRAINT bank_statements_source
        CHECK (source IN ('camt', 'mt940', 'csv')),
    CONSTRAINT bank_statements_sha_shape
        CHECK (file_sha256 ~ '^[0-9a-f]{64}$'),
    CONSTRAINT bank_statements_currency_shape
        CHECK (currency ~ '^[A-Z]{3}$'),
    CONSTRAINT bank_statements_iban_shape
        CHECK (char_length(account_iban) <= 34),
    CONSTRAINT bank_statements_ref_shape
        CHECK (char_length(statement_ref) <= 140),
    -- A period that ends before it starts is not a period.
    CONSTRAINT bank_statements_period CHECK (to_date >= from_date)
);

-- The list screen: this tenant's imports, newest period first.
CREATE INDEX bank_statements_by_period
    ON bank_statements (tenant_id, to_date DESC, imported_at DESC);

CREATE TABLE bank_lines (
    tenant_id          TEXT NOT NULL,
    id                 TEXT NOT NULL,
    statement_id       TEXT NOT NULL,
    -- Where in the file this line was, from 1. The order a bookkeeper reads
    -- the statement in, and the tie-break that keeps two identical entries on
    -- one day in the order the bank listed them.
    line_no            INTEGER NOT NULL,
    -- Booked is the day the bank posted it and the day the books use; value is
    -- the day interest counts from, which is sometimes earlier and is the one
    -- the customer thinks they paid on. Both are kept: matching a payment to an
    -- invoice within a date window needs the second as often as the first.
    booked_on          DATE NOT NULL,
    value_on           DATE NOT NULL,
    -- SIGNED, in integer cents, in this line's own currency: positive is money
    -- in, negative is money out. The wire formats do not say it this way (CAMT
    -- states a positive amount beside a credit/debit indicator, MT940 a `C` or
    -- `D` in the line), and normalising the sign at the parser is what keeps
    -- every reader after it from re-deciding which way a number points.
    amount_cents       BIGINT NOT NULL,
    -- The line's own currency, which is usually but not always the statement's:
    -- a euro account can carry a line the bank states in another currency.
    currency           TEXT NOT NULL,
    -- Who the money came from (on a credit) or went to (on a debit) — never the
    -- account holder themselves. Descriptive, best-effort, and blank on a
    -- batched entry that has no single counterparty.
    counterparty_name  TEXT NOT NULL DEFAULT '',
    counterparty_iban  TEXT NOT NULL DEFAULT '',
    -- What the payer wrote on it. The single most load-bearing field in the
    -- whole table: B4.09's exact stage matches an invoice by finding our own
    -- number in here.
    remittance         TEXT NOT NULL DEFAULT '',
    -- The bank's own reference for the entry, when it states one.
    bank_ref           TEXT NOT NULL DEFAULT '',
    -- The line-level duplicate rule: SHA-256 over the normalised (account,
    -- booked date, signed amount, currency, bank reference, counterparty IBAN,
    -- remittance) plus an occurrence number that tells two genuinely identical
    -- transactions on one day apart. See `bank_import.rs`.
    line_hash          TEXT NOT NULL,
    -- 'unmatched' until a human says otherwise (B4.09): 'matched' once a
    -- confirmed match exists, 'ignored' when it is not ours to book.
    status             TEXT NOT NULL DEFAULT 'unmatched',
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- The statement is the parent, and deleting an import takes its staged
    -- lines with it. The tenant travels in the key so a line can never point at
    -- another tenant's statement.
    FOREIGN KEY (tenant_id, statement_id)
        REFERENCES bank_statements (tenant_id, id) ON DELETE CASCADE,
    UNIQUE (tenant_id, line_hash),
    CONSTRAINT bank_lines_status
        CHECK (status IN ('unmatched', 'matched', 'ignored')),
    CONSTRAINT bank_lines_hash_shape
        CHECK (line_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT bank_lines_currency_shape
        CHECK (currency ~ '^[A-Z]{3}$'),
    CONSTRAINT bank_lines_no_positive CHECK (line_no >= 1),
    -- Zero is not a transaction, and the ceiling is the typo guard every alo
    -- money column carries (±10 billion cents).
    CONSTRAINT bank_lines_amount_range
        CHECK (amount_cents <> 0 AND abs(amount_cents) <= 1000000000000),
    CONSTRAINT bank_lines_text_shape CHECK (
        char_length(counterparty_name) <= 140
        AND char_length(counterparty_iban) <= 34
        AND char_length(remittance) <= 1000
        AND char_length(bank_ref) <= 140
    )
);

-- The reconciliation screen: this tenant's lines, optionally one statement's,
-- optionally one status, oldest first (a bookkeeper works forwards through a
-- month).
CREATE INDEX bank_lines_by_status
    ON bank_lines (tenant_id, status, booked_on, line_no);
CREATE INDEX bank_lines_by_statement
    ON bank_lines (tenant_id, statement_id, line_no);
