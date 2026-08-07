-- alo Billing (ADR 0035, wave B1.21): invoicing in more than one currency.
--
-- Three additive changes, in the order they depend on each other:
--
--   1. the currency the tenant keeps books in (`billing_settings.base_currency`)
--   2. the euro reference rates it has imported (`billing_fx_rates`)
--   3. the rate SNAPSHOT frozen on a document when it is issued
--
-- Why a snapshot rather than a lookup at read time: EU VAT Directive art. 91
-- fixes the conversion rate at the moment the tax becomes chargeable — the
-- issue date, under the ordinary invoice-based scheme — and art. 230 requires
-- the VAT amount to be expressed in the member state's own currency. Both are
-- facts about the document, so both are ON the document, exactly like its
-- number and its dates. Re-deriving the rate at read time would silently
-- restate last year's invoice at today's rate the moment a rate row is
-- corrected.
--
-- Why the rate table is TENANT-SCOPED even though the published rates are a
-- public fact: a tenant imports the file it will be audited against, and some
-- member states (and some contracts) require a different published series than
-- the ECB's. A shared table would make one tenant's import change another
-- tenant's books — precisely what law 1 exists to prevent — and the volume is
-- trivial (about thirty rates per working day).
--
-- Rates are INTEGER micro-units of the quoted currency per one unit of the base
-- currency, the direction the ECB publishes in (1 EUR = 1.162600 USD is
-- 1162600). No column in alo Billing is ever floating point, a rate least of
-- all: it multiplies every amount on the document.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

-- 1. The issuer's accounting currency. Documents may be raised in any currency;
-- this is the one the VAT return, the ledger (B4) and the printed VAT total are
-- expressed in. Defaulted rather than nullable: a tenant that has never opened
-- the settings screen still keeps books in something, and for a European
-- product that is the euro until it says otherwise.
ALTER TABLE billing_settings
    ADD COLUMN base_currency TEXT NOT NULL DEFAULT 'EUR';

ALTER TABLE billing_settings
    ADD CONSTRAINT billing_settings_base_currency_shape
        CHECK (base_currency ~ '^[A-Z]{3}$');

-- 2. The reference rates a tenant has imported: one row per currency per
-- published day. Imported from the ECB's own file (`billing_fx_ecb.rs`) or
-- entered by hand; re-importing a day overwrites it, which is how a published
-- correction lands.
CREATE TABLE billing_fx_rates (
    tenant_id  TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    -- ISO 4217 of the quoted currency, uppercased in the store. The base of the
    -- quote is the euro — the currency the published file quotes against — not
    -- the tenant's own base currency: a non-euro issuer's rates are CROSSED
    -- from these two euro quotes when a document is issued, and the cross is
    -- what gets snapshotted (billing_fx.rs).
    currency   TEXT NOT NULL,
    -- The day the rate was published, not the day it was imported.
    rate_date  DATE NOT NULL,
    -- Micro-units of `currency` per one euro. 1 EUR = 1.1626 USD is 1162600.
    rate_micro BIGINT NOT NULL,
    -- Where the row came from, for the audit trail an import needs: 'ecb' for a
    -- parsed reference-rate file, 'manual' for a hand-entered rate.
    source     TEXT NOT NULL,
    updated_by TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, currency, rate_date),
    -- Defence in depth: the store validates all of this before writing, so a
    -- violation here means a bug, not user input.
    CONSTRAINT billing_fx_rates_currency_shape CHECK (currency ~ '^[A-Z]{3}$'),
    -- Strictly positive: a zero rate would divide every amount by nothing, and
    -- an unknown rate is an absent row rather than a stored zero. The ceiling
    -- keeps `cents × 1e6 / rate` inside i128 for any storable amount.
    CONSTRAINT billing_fx_rates_range
        CHECK (rate_micro >= 1 AND rate_micro <= 1000000000000000),
    CONSTRAINT billing_fx_rates_source_known CHECK (source IN ('ecb', 'manual'))
);

-- The one question a document asks at issue: "the newest rate for this currency
-- at or before this day" (art. 91's "last preceding date of publication").
CREATE INDEX billing_fx_rates_by_day
    ON billing_fx_rates (tenant_id, currency, rate_date DESC);

-- 3. The snapshot on the document. All three columns move together: the
-- currency the amounts were restated into, the rate applied, and the day that
-- rate was published.
ALTER TABLE billing_invoices
    ADD COLUMN fx_base_currency TEXT,
    ADD COLUMN fx_rate_micro    BIGINT,
    ADD COLUMN fx_rate_date     DATE;

-- Every document issued before this migration was raised in a single-currency
-- tenant, so its rate is the identity — but only where that is DEMONSTRABLY
-- true: where the document's own currency is the tenant's base currency. A
-- foreign-currency document from before there were rates keeps a NULL snapshot
-- and is reported as unconverted rather than being assigned a rate of 1, which
-- would be a made-up figure on a tax return.
UPDATE billing_invoices i
   SET fx_base_currency = i.currency,
       fx_rate_micro    = 1000000,
       fx_rate_date     = i.issue_date
 WHERE i.status <> 'draft'
   AND i.issue_date IS NOT NULL
   AND i.currency = COALESCE(
           (SELECT s.base_currency FROM billing_settings s WHERE s.tenant_id = i.tenant_id),
           'EUR');

ALTER TABLE billing_invoices
    -- All three or none: a rate without the currency it converts into, or
    -- without the day it was published on, is not a snapshot anybody can audit.
    ADD CONSTRAINT billing_invoices_fx_whole
        CHECK (num_nulls(fx_base_currency, fx_rate_micro, fx_rate_date) IN (0, 3)),
    -- A draft carries no snapshot, for the same reason it carries no number:
    -- the rate belongs to the moment the document became a document. (The
    -- converse is deliberately NOT asserted — a legacy issued document may
    -- carry none, see the backfill above.)
    ADD CONSTRAINT billing_invoices_fx_not_on_a_draft
        CHECK (status <> 'draft' OR fx_rate_micro IS NULL),
    ADD CONSTRAINT billing_invoices_fx_base_currency_shape
        CHECK (fx_base_currency IS NULL OR fx_base_currency ~ '^[A-Z]{3}$'),
    ADD CONSTRAINT billing_invoices_fx_rate_range
        CHECK (fx_rate_micro IS NULL
               OR (fx_rate_micro >= 1 AND fx_rate_micro <= 1000000000000000));
