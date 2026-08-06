-- alo Billing (ADR 0035, wave B1): the customers a tenant invoices.
-- Tenant-scoped and cascading with the tenant (Law 1). Customers are
-- tenant-wide — every user of the tenant bills the same customer list, like
-- `sites` — and are ARCHIVED, never deleted: an issued invoice must always be
-- able to name its customer (docs/design/billing.md).
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE billing_customers (
    tenant_id          TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id                 TEXT NOT NULL,
    -- Legal/display name of the company or person invoiced.
    name               TEXT NOT NULL,
    address_line1      TEXT NOT NULL DEFAULT '',
    address_line2      TEXT NOT NULL DEFAULT '',
    postal_code        TEXT NOT NULL DEFAULT '',
    city               TEXT NOT NULL DEFAULT '',
    -- ISO 3166-1 alpha-2, uppercased in the store. Required: it drives the
    -- VAT treatment of every document raised for this customer.
    country            TEXT NOT NULL,
    -- VAT identification number, NULL for B2C customers. Stored compact and
    -- uppercased; per-country format validation lands with B1.03.
    vat_id             TEXT,
    -- Where invoices are sent (B1.18 drafts the mail); NULL when unknown.
    email              TEXT,
    -- Days from issue date to due date, snapshotted onto each invoice.
    payment_terms_days INTEGER NOT NULL DEFAULT 30,
    -- ISO 4217, uppercased. The default for documents raised for this
    -- customer; the invoice keeps its own copy (B1.06).
    currency           TEXT NOT NULL DEFAULT 'EUR',
    -- Optional link to an address-book contact. Deleting the contact unlinks
    -- rather than destroying billing history.
    contact_id         TEXT,
    -- NULL = active. Archiving hides the customer from pickers without
    -- breaking any document that names it.
    archived_at        TIMESTAMPTZ,
    created_by         TEXT NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    CONSTRAINT billing_customers_contact_fk
        FOREIGN KEY (contact_id) REFERENCES contacts (id) ON DELETE SET NULL,
    -- Defence in depth: the store validates all three before writing, so a
    -- violation here means a bug, not user input.
    CONSTRAINT billing_customers_name_shape CHECK (length(btrim(name)) > 0),
    CONSTRAINT billing_customers_country_shape CHECK (country ~ '^[A-Z]{2}$'),
    CONSTRAINT billing_customers_currency_shape CHECK (currency ~ '^[A-Z]{3}$'),
    CONSTRAINT billing_customers_terms_range
        CHECK (payment_terms_days >= 0 AND payment_terms_days <= 365)
);

-- The list surface: a tenant's customers in name order, active ones first.
CREATE INDEX billing_customers_by_name ON billing_customers (tenant_id, lower(name));
