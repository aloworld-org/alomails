-- alo Billing (ADR 0035, wave B1): invoices and their lines.
--
-- An invoice is a document, not a row that gets edited forever. While it is a
-- DRAFT it is freely editable and carries no number; ISSUING it (B1.08) draws
-- the next number from the tenant's gapless sequence, stamps the dates and
-- freezes the content. That lifecycle is why the constraints below tie
-- `number`, `issue_date` and `due_date` to the status rather than leaving them
-- independently nullable: a numbered draft or an issued document without a
-- number is not a state this business can be in.
--
-- Lines SNAPSHOT the price list (docs/design/billing.md): description, unit,
-- unit price and VAT rate are copied onto the line when a product is picked,
-- and there is deliberately NO foreign key back to `billing_products` — a
-- later price change must never rewrite a document that was already raised.
--
-- Money is integer cents, quantities are milli-units (1.5 h = 1500) and VAT
-- rates are basis points (2100 = 21 %). No column in alo Billing is ever
-- floating point.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE billing_invoices (
    tenant_id          TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id                 TEXT NOT NULL,
    -- The party billed. The composite reference pins the customer to the SAME
    -- tenant at the database level, so even a bug in a WHERE clause cannot
    -- link across tenants. Customers are archived rather than deleted, so the
    -- cascade here only ever fires when the whole tenant is deleted.
    customer_id        TEXT NOT NULL,
    -- draft → issued → paid, or issued → void. Only a draft is editable.
    status             TEXT NOT NULL DEFAULT 'draft',
    -- ISO 4217, snapshotted from the customer when the draft is raised: the
    -- document carries the currency it was raised in, not the one the customer
    -- happens to have today.
    currency           TEXT NOT NULL DEFAULT 'EUR',
    -- NULL while draft, so an abandoned draft can never consume a number
    -- (the legal gapless-numbering rule, docs/design/billing.md).
    number             TEXT,
    issue_date         DATE,
    due_date           DATE,
    -- Snapshot of the customer's terms, in days, used to derive the due date
    -- at issue. Kept on the document because the customer's terms may change
    -- after this invoice was raised.
    payment_terms_days INTEGER NOT NULL DEFAULT 30,
    -- A credit note is an invoice with negative lines that names the document
    -- it credits (B1.09); it draws from the same number sequence so the ledger
    -- stays continuous.
    is_credit_note     BOOLEAN NOT NULL DEFAULT false,
    credits_invoice_id TEXT,
    -- The customer's own reference (PO number) and a free-text note, both
    -- printed on the document.
    reference          TEXT NOT NULL DEFAULT '',
    note               TEXT NOT NULL DEFAULT '',
    created_by         TEXT NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, customer_id)
        REFERENCES billing_customers (tenant_id, id) ON DELETE CASCADE,
    -- A credit note names a document of the same tenant. NULL is unconstrained
    -- (MATCH SIMPLE), which is exactly right for an ordinary invoice.
    CONSTRAINT billing_invoices_credits_fk FOREIGN KEY (tenant_id, credits_invoice_id)
        REFERENCES billing_invoices (tenant_id, id),
    -- Defence in depth: the store validates all of this before writing, so a
    -- violation here means a bug in our code, not bad user input.
    CONSTRAINT billing_invoices_status_known
        CHECK (status IN ('draft', 'issued', 'paid', 'void')),
    -- Numbers and dates exist exactly when the document is no longer a draft.
    CONSTRAINT billing_invoices_number_iff_issued
        CHECK ((status = 'draft') = (number IS NULL)),
    CONSTRAINT billing_invoices_dates_iff_issued
        CHECK ((status = 'draft') = (issue_date IS NULL)
               AND (status = 'draft') = (due_date IS NULL)),
    CONSTRAINT billing_invoices_credit_note_names_its_original
        CHECK (is_credit_note = (credits_invoice_id IS NOT NULL)),
    CONSTRAINT billing_invoices_currency_shape
        CHECK (currency ~ '^[A-Z]{3}$'),
    CONSTRAINT billing_invoices_terms_range
        CHECK (payment_terms_days >= 0 AND payment_terms_days <= 365)
);

-- One number per tenant, ever. Postgres allows many NULLs in a unique index,
-- so unnumbered drafts never collide with each other.
CREATE UNIQUE INDEX billing_invoices_number_unique
    ON billing_invoices (tenant_id, number);
-- "Everything I billed this customer" and the status-filtered list surface.
CREATE INDEX billing_invoices_by_customer
    ON billing_invoices (tenant_id, customer_id, created_at DESC);
CREATE INDEX billing_invoices_by_status
    ON billing_invoices (tenant_id, status, created_at DESC);

CREATE TABLE billing_invoice_lines (
    tenant_id        TEXT NOT NULL,
    invoice_id       TEXT NOT NULL,
    id               TEXT NOT NULL,
    -- Position on the printed document, 0-based and contiguous: the store
    -- replaces the whole line set in one transaction rather than patching
    -- individual rows, so the order is always exactly the caller's order.
    line_order       INTEGER NOT NULL,
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
    -- through its invoice, which is the single place that link is stated.
    FOREIGN KEY (tenant_id, invoice_id)
        REFERENCES billing_invoices (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT billing_invoice_lines_description_shape
        CHECK (length(btrim(description)) > 0),
    CONSTRAINT billing_invoice_lines_order_range CHECK (line_order >= 0),
    -- The bounds that keep every total inside i64 (see billing_line.rs):
    -- |qty| ≤ 10^9 milli-units × price ≤ 10^9 cents × 500 lines.
    CONSTRAINT billing_invoice_lines_qty_range
        CHECK (qty_milli >= -1000000000 AND qty_milli <= 1000000000),
    CONSTRAINT billing_invoice_lines_price_range
        CHECK (unit_price_cents >= 0 AND unit_price_cents <= 1000000000),
    CONSTRAINT billing_invoice_lines_vat_rate_range
        CHECK (vat_rate_bp >= 0 AND vat_rate_bp <= 10000)
);

-- The document read: every line of one invoice, in print order.
CREATE UNIQUE INDEX billing_invoice_lines_in_order
    ON billing_invoice_lines (tenant_id, invoice_id, line_order);
