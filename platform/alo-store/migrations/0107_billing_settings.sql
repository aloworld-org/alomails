-- alo Billing (ADR 0035, wave B1): the issuer side of every document.
--
-- Who is billing, under which VAT and registration numbers, and where the
-- money goes. A customer record says who a document is TO; this says who it
-- is FROM, and it is the same for every document a tenant raises — so it is
-- ONE ROW PER TENANT (the primary key is the tenant), not a per-document
-- snapshot and not a per-user preference.
--
-- A tenant that has never saved has no row and reads blanks
-- (docs/design/billing.md): the record conceptually always exists, so the
-- print view never has to ask whether billing has been "set up".
--
-- Nothing here is copied onto a document at issue time in B1. An invoice
-- already frozen therefore reprints with the CURRENT issuer details — which
-- is what a change of bank account or address is supposed to do, and is why
-- the legal identifiers that must not drift (the number, the dates, the
-- lines, the money) live on the document itself.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE billing_settings (
    tenant_id       TEXT PRIMARY KEY REFERENCES tenants (id) ON DELETE CASCADE,
    -- The legal name the tenant invoices under. Required on every save: a
    -- document that does not name its issuer is not an invoice.
    legal_name      TEXT NOT NULL,
    address_line1   TEXT NOT NULL DEFAULT '',
    address_line2   TEXT NOT NULL DEFAULT '',
    postal_code     TEXT NOT NULL DEFAULT '',
    city            TEXT NOT NULL DEFAULT '',
    -- ISO 3166-1 alpha-2, or blank while unstated. Blank is allowed here
    -- (unlike on a customer, where it drives VAT treatment) so a tenant can
    -- save a name and an address before deciding anything fiscal.
    country         TEXT NOT NULL DEFAULT '',
    -- Canonical prefixed form (DE811907980); NULL for a tenant not
    -- VAT-registered, which is a real state for a small trader.
    vat_id          TEXT,
    -- Company/commercial register number as printed (KVK, SIREN, HRB …).
    -- Free text: every member state numbers its register differently.
    registration_no TEXT NOT NULL DEFAULT '',
    email           TEXT NOT NULL DEFAULT '',
    phone           TEXT NOT NULL DEFAULT '',
    website         TEXT NOT NULL DEFAULT '',
    -- Where the money goes. The IBAN is stored compacted and uppercase,
    -- validated for its country's length and its mod-97 check digits; NULL
    -- when unstated. The BIC is 8 or 11 characters, uppercase.
    iban            TEXT,
    bic             TEXT,
    -- Bank name and account holder as they should be printed, when they are
    -- not simply the legal name (a trading name, a factoring account).
    bank_name       TEXT NOT NULL DEFAULT '',
    account_holder  TEXT NOT NULL DEFAULT '',
    -- A line under the totals: retention of title, late-payment terms, a
    -- thank-you. Printed verbatim on every document.
    footer_note     TEXT NOT NULL DEFAULT '',
    updated_by      TEXT NOT NULL,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Defence in depth: the store validates before writing, so a violation
    -- here means a bug, not user input.
    CONSTRAINT billing_settings_legal_name_shape
        CHECK (length(btrim(legal_name)) > 0),
    CONSTRAINT billing_settings_country_shape
        CHECK (country = '' OR country ~ '^[A-Z]{2}$'),
    CONSTRAINT billing_settings_iban_shape
        CHECK (iban IS NULL OR iban ~ '^[A-Z]{2}[0-9]{2}[A-Z0-9]{1,30}$'),
    CONSTRAINT billing_settings_bic_shape
        CHECK (bic IS NULL OR bic ~ '^[A-Z0-9]{8}([A-Z0-9]{3})?$')
);
