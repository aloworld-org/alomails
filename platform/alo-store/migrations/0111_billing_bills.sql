-- alo Billing (ADR 0035, wave B1.24): a bill — somebody else's invoice, read
-- from the e-invoice file they sent us, waiting to be approved.
--
-- This is the mirror of `billing_invoices`, and it is deliberately a DIFFERENT
-- table rather than a flag on that one. An invoice is a document we raise, own
-- and are legally answerable for: it draws a number from our gapless series, it
-- freezes on issue, it is what a tax inspection reads. A bill is a document
-- somebody else raised: it carries THEIR number, THEIR dates and THEIR totals,
-- and the only thing we decide about it is whether we accept it. Putting both
-- in one table would put a foreign number in the column our own series lives
-- in, and every rule about issuing, voiding and crediting would need "unless it
-- is theirs" written into it.
--
-- **What is stored is what the document says.** The totals below are the
-- supplier's own figures (BT-106 … BT-115), copied across, not recomputed: a
-- bill's authority is the file the supplier sent, and a stored figure that
-- disagreed with their paper would be our arithmetic quietly overruling their
-- invoice. The import nevertheless RECONCILES them before writing (the lines
-- must sum to the stated line total, and the standard's own equations must
-- hold), so an incoherent document is refused at the door instead of being
-- booked and discovered at the year end.
--
-- **A credit note is stored in ledger direction** — negative — exactly as our
-- own credit notes are (B1.09). The file states type 381 with positive amounts;
-- the sign is flipped once, on the way in, so that a bill and the credit note
-- against it sum to zero in the accounts (B4) without every reader having to
-- know the standard's convention.
--
-- Money is integer cents, quantities milli-units, VAT rates basis points —
-- the same units as everywhere else in billing, so a line of theirs and a line
-- of ours are the same shape.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE billing_bills (
    tenant_id             TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id                    TEXT NOT NULL,
    -- Which syntax the document arrived in: 'cii' (Factur-X/ZUGFeRD) or 'ubl'
    -- (XRechnung/Peppol). Kept because the two are the same invoice written
    -- down two ways and a support question always starts with which one.
    source_syntax         TEXT NOT NULL,
    -- SHA-256 of the imported bytes, lower-case hex. The original file itself
    -- is NOT stored here (archiving it is a Drive concern, not a column): the
    -- hash is what will tie this record to that archive, and what proves the
    -- same file twice is the same file.
    source_sha256         TEXT NOT NULL,
    -- UNTDID 1001 (BT-3): '380' a commercial invoice, '381' a credit note.
    type_code             TEXT NOT NULL,
    -- Where the approval stands: 'received' (nobody has decided),
    -- 'approved' (we accept it — the liability is real), 'rejected' (we do
    -- not). A decision is final; correcting one means the supplier credits
    -- their document, which arrives as a bill of its own.
    status                TEXT NOT NULL DEFAULT 'received',
    -- The supplier, copied from the document. No foreign key: a supplier
    -- record is B5.03, and a bill must stay readable whatever happens to any
    -- master data we later keep about them.
    supplier_name         TEXT NOT NULL,
    supplier_vat_id       TEXT NOT NULL DEFAULT '',
    supplier_legal_id     TEXT NOT NULL DEFAULT '',
    supplier_line1        TEXT NOT NULL DEFAULT '',
    supplier_line2        TEXT NOT NULL DEFAULT '',
    supplier_postal_code  TEXT NOT NULL DEFAULT '',
    supplier_city         TEXT NOT NULL DEFAULT '',
    supplier_country      TEXT NOT NULL DEFAULT '',
    supplier_email        TEXT NOT NULL DEFAULT '',
    supplier_iban         TEXT NOT NULL DEFAULT '',
    -- Who the document is FROM, reduced to one comparable key: the supplier's
    -- VAT identifier when they state one, otherwise their name folded to lower
    -- case. It exists for the uniqueness constraint below and nothing else.
    supplier_key          TEXT NOT NULL,
    -- Their number (BT-1) and their dates (BT-2, BT-9). `number` is theirs
    -- alone: nothing in our own numbering ever reads this column.
    number                TEXT NOT NULL,
    issue_date            DATE NOT NULL,
    due_date              DATE,
    currency              TEXT NOT NULL,
    -- Their reference for us (BT-10), their note (BT-22), and the remittance
    -- reference to quote when paying (BT-83).
    buyer_reference       TEXT NOT NULL DEFAULT '',
    note                  TEXT NOT NULL DEFAULT '',
    payment_reference     TEXT NOT NULL DEFAULT '',
    -- The stated totals, in ledger direction. BT-106 … BT-115.
    line_total_cents      BIGINT NOT NULL,
    allowance_total_cents BIGINT NOT NULL DEFAULT 0,
    charge_total_cents    BIGINT NOT NULL DEFAULT 0,
    tax_exclusive_cents   BIGINT NOT NULL,
    tax_total_cents       BIGINT NOT NULL,
    tax_inclusive_cents   BIGINT NOT NULL,
    prepaid_cents         BIGINT NOT NULL DEFAULT 0,
    payable_cents         BIGINT NOT NULL,
    imported_by           TEXT NOT NULL,
    imported_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    decided_by            TEXT,
    decided_at            TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, id),
    -- The same document must not be booked twice. A supplier's number is
    -- unique within that supplier by law, so (supplier, number) is the
    -- document's identity — and it catches the duplicate that matters, the
    -- same invoice forwarded twice and imported by two people, which a hash
    -- alone would miss the moment the file is re-exported.
    CONSTRAINT billing_bills_one_per_supplier_number
        UNIQUE (tenant_id, supplier_key, number),
    -- Defence in depth: the store validates all of this before writing, so a
    -- violation here means a bug in our code, not a bad upload.
    CONSTRAINT billing_bills_syntax_known
        CHECK (source_syntax IN ('cii', 'ubl')),
    CONSTRAINT billing_bills_type_known
        CHECK (type_code IN ('380', '381')),
    CONSTRAINT billing_bills_status_known
        CHECK (status IN ('received', 'approved', 'rejected')),
    CONSTRAINT billing_bills_supplier_named
        CHECK (length(btrim(supplier_name)) > 0 AND length(supplier_name) <= 200),
    CONSTRAINT billing_bills_numbered
        CHECK (length(btrim(number)) > 0 AND length(number) <= 60),
    CONSTRAINT billing_bills_supplier_keyed
        CHECK (length(supplier_key) > 0),
    CONSTRAINT billing_bills_currency_shape
        CHECK (currency ~ '^[A-Z]{3}$'),
    CONSTRAINT billing_bills_hashed
        CHECK (source_sha256 ~ '^[0-9a-f]{64}$'),
    -- An undecided bill has no decision on it, and a decided one names who
    -- made it and when. The two can never be half-set.
    CONSTRAINT billing_bills_decision_complete
        CHECK ((status = 'received') = (decided_at IS NULL)
               AND (decided_at IS NULL) = (decided_by IS NULL))
);

-- The approval queue: what is waiting, newest document first. Also the read
-- behind "everything from this year" once a status filter is applied.
CREATE INDEX billing_bills_by_status
    ON billing_bills (tenant_id, status, issue_date DESC, id);

-- What we owe, and to whom: the read the aged-payables report (B4.11) and the
-- SEPA export of approved bills (B2.12) both start from.
CREATE INDEX billing_bills_by_supplier
    ON billing_bills (tenant_id, supplier_key, issue_date DESC);

-- The lines of a bill: the same shape as `billing_invoice_lines`, because a
-- line of theirs and a line of ours are the same thing — that is what lets one
-- line module (`platform/alo-store/src/billing_line.rs`) read and write both.
-- Lines reach their tenant only through their bill, and go with it.
CREATE TABLE billing_bill_lines (
    tenant_id        TEXT NOT NULL,
    bill_id          TEXT NOT NULL,
    id               TEXT NOT NULL,
    line_order       INTEGER NOT NULL,
    description      TEXT NOT NULL,
    unit             TEXT NOT NULL DEFAULT '',
    qty_milli        BIGINT NOT NULL,
    unit_price_cents BIGINT NOT NULL,
    vat_rate_bp      INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, bill_id)
        REFERENCES billing_bills (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT billing_bill_lines_described
        CHECK (length(btrim(description)) > 0 AND length(description) <= 200),
    CONSTRAINT billing_bill_lines_order_range CHECK (line_order >= 0),
    CONSTRAINT billing_bill_lines_price_sane
        CHECK (unit_price_cents >= 0 AND unit_price_cents <= 1000000000),
    CONSTRAINT billing_bill_lines_rate_sane
        CHECK (vat_rate_bp >= 0 AND vat_rate_bp <= 10000),
    CONSTRAINT billing_bill_lines_qty_sane
        CHECK (qty_milli >= -1000000000 AND qty_milli <= 1000000000)
);

CREATE INDEX billing_bill_lines_by_bill
    ON billing_bill_lines (tenant_id, bill_id, line_order);
