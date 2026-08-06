-- alo Billing (ADR 0035, wave B1): quotes and their lines.
--
-- A quote is the offer that precedes an invoice, and it is the SAME kind of
-- object: a draft that is freely editable and carries no number, then a
-- document that left the building and is frozen. SENDING it draws the next
-- number from the tenant's `quote` series, stamps the send date and derives
-- the validity date from the days snapshotted on the document — exactly the
-- shape issuing gives an invoice (0102), which is why the constraints below
-- tie `number`, `sent_date` and `valid_until` to the status rather than
-- leaving them independently nullable.
--
-- WHY A SECOND TABLE AND NOT A `kind` COLUMN ON `billing_invoices`: the two
-- documents share a line model, not a life. An invoice is owed money with a
-- legally gapless number and a due date; a quote is an offer with a validity
-- date that ends in accepted/declined/expired and owes nothing. Folding them
-- together would make every invoice query filter on a discriminator and would
-- put a quote's states inside the CHECK that guards invoice numbering. The
-- LINE model is shared in code (`billing_line.rs`) — that is where the real
-- duplication would have been.
--
-- Quote numbers are not the legal gapless series invoices need, but they are
-- drawn the same transactional way (`billing_sequences`, kind 'quote') so a
-- customer never receives two offers bearing one number.
--
-- Money is integer cents, quantities are milli-units (1.5 h = 1500) and VAT
-- rates are basis points (2100 = 21 %). No column in alo Billing is ever
-- floating point.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE billing_quotes (
    tenant_id     TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id            TEXT NOT NULL,
    -- The party quoted. The composite reference pins the customer to the SAME
    -- tenant at the database level, so even a bug in a WHERE clause cannot
    -- link across tenants.
    customer_id   TEXT NOT NULL,
    -- draft → sent → accepted | declined | expired. Only a draft is editable,
    -- and only a draft is deleted (it never consumed a number).
    status        TEXT NOT NULL DEFAULT 'draft',
    -- ISO 4217, snapshotted from the customer when the draft is raised.
    currency      TEXT NOT NULL DEFAULT 'EUR',
    -- NULL while draft: an abandoned draft never consumes a number.
    number        TEXT,
    sent_date     DATE,
    valid_until   DATE,
    -- How long the offer stands, in days from the send date. Kept on the
    -- document (like an invoice's payment terms) so a later change to the
    -- tenant's habits cannot restate an offer already made.
    valid_days    INTEGER NOT NULL DEFAULT 30,
    -- The day the offer was accepted, declined or expired — NULL until it is
    -- one of those. It is the answer to "when did this stop being open?",
    -- which `updated_at` cannot give (any later touch moves that).
    decided_date  DATE,
    -- The customer's own reference (RFQ number) and a free-text note, both
    -- printed on the document.
    reference     TEXT NOT NULL DEFAULT '',
    note          TEXT NOT NULL DEFAULT '',
    created_by    TEXT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, customer_id)
        REFERENCES billing_customers (tenant_id, id) ON DELETE CASCADE,
    -- Defence in depth: the store validates all of this before writing, so a
    -- violation here means a bug in our code, not bad user input.
    CONSTRAINT billing_quotes_status_known
        CHECK (status IN ('draft', 'sent', 'accepted', 'declined', 'expired')),
    -- A number and the two dates exist exactly when the quote is no longer a
    -- draft — sending is the only transition that assigns them, and no later
    -- transition clears them.
    CONSTRAINT billing_quotes_number_iff_sent
        CHECK ((status = 'draft') = (number IS NULL)),
    CONSTRAINT billing_quotes_dates_iff_sent
        CHECK ((status = 'draft') = (sent_date IS NULL)
               AND (status = 'draft') = (valid_until IS NULL)),
    -- A decision date exists exactly when the offer is closed. `sent` is open
    -- and `draft` was never an offer; the other three are decided.
    CONSTRAINT billing_quotes_decided_iff_closed
        CHECK ((status IN ('accepted', 'declined', 'expired'))
               = (decided_date IS NOT NULL)),
    -- The offer can never expire before it was made.
    CONSTRAINT billing_quotes_valid_until_after_sent
        CHECK (valid_until IS NULL OR sent_date IS NULL OR valid_until >= sent_date),
    CONSTRAINT billing_quotes_currency_shape
        CHECK (currency ~ '^[A-Z]{3}$'),
    CONSTRAINT billing_quotes_valid_days_range
        CHECK (valid_days >= 0 AND valid_days <= 365)
);

-- One number per tenant, ever. Postgres allows many NULLs in a unique index,
-- so unnumbered drafts never collide with each other.
CREATE UNIQUE INDEX billing_quotes_number_unique
    ON billing_quotes (tenant_id, number);
-- "Everything I offered this customer" and the status-filtered list surface.
CREATE INDEX billing_quotes_by_customer
    ON billing_quotes (tenant_id, customer_id, created_at DESC);
CREATE INDEX billing_quotes_by_status
    ON billing_quotes (tenant_id, status, created_at DESC);

CREATE TABLE billing_quote_lines (
    tenant_id        TEXT NOT NULL,
    quote_id         TEXT NOT NULL,
    id               TEXT NOT NULL,
    -- Position on the printed document, 0-based and contiguous: the store
    -- replaces the whole line set in one transaction rather than patching
    -- individual rows, so the order is always exactly the caller's order.
    line_order       INTEGER NOT NULL,
    -- Lines SNAPSHOT the price list, exactly as invoice lines do: no foreign
    -- key back to `billing_products`, so a later price change never rewrites
    -- an offer already made — and the copy an accepted quote makes into an
    -- invoice draft (B1.12) is a copy of these frozen values.
    description      TEXT NOT NULL,
    unit             TEXT NOT NULL DEFAULT '',
    -- Quantity in milli-units: 1.5 hours = 1500. NEGATIVE IS LEGITIMATE — it
    -- is how a discount line is expressed (a negative unit price is not, see
    -- billing_field.rs).
    qty_milli        BIGINT NOT NULL,
    unit_price_cents BIGINT NOT NULL,
    vat_rate_bp      INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, id),
    -- No tenants(id) reference of its own: a line reaches its tenant only
    -- through its quote, which is the single place that link is stated.
    FOREIGN KEY (tenant_id, quote_id)
        REFERENCES billing_quotes (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT billing_quote_lines_description_shape
        CHECK (length(btrim(description)) > 0),
    CONSTRAINT billing_quote_lines_order_range CHECK (line_order >= 0),
    -- The bounds that keep every total inside i64 (see billing_line.rs):
    -- |qty| ≤ 10^9 milli-units × price ≤ 10^9 cents × 500 lines.
    CONSTRAINT billing_quote_lines_qty_range
        CHECK (qty_milli >= -1000000000 AND qty_milli <= 1000000000),
    CONSTRAINT billing_quote_lines_price_range
        CHECK (unit_price_cents >= 0 AND unit_price_cents <= 1000000000),
    CONSTRAINT billing_quote_lines_vat_rate_range
        CHECK (vat_rate_bp >= 0 AND vat_rate_bp <= 10000)
);

-- The document read: every line of one quote, in print order.
CREATE UNIQUE INDEX billing_quote_lines_in_order
    ON billing_quote_lines (tenant_id, quote_id, line_order);
