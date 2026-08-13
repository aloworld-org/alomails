-- Buying a domain through alo: one row per purchase, from the price the
-- tenant was shown to the name answering for their site (ADR 0036, S2.15b).
--
-- The row is the state machine. `state` moves only forwards and only through
-- the transitions `site_domain_purchases.rs` allows: quoted → approved →
-- awaiting_payment → paid → registering → registered → configured, with
-- `cancelled` reachable only before money moved and `failed` only from a
-- registrar refusal. Nothing here is a queue of intentions kept beside the
-- truth: the registrar's own answer (provider_reference, expires_at,
-- lifecycle) is written back onto the same row.
--
-- Money is frozen at the quote. `first_term_cents` is what the tenant pays
-- now for `term_years` years and `renewal_cents_per_year` what it costs
-- afterwards, both VAT-exclusive and both stated before approval — the
-- honest-pricing promise of `site_registrar.rs` is worth nothing if the
-- number moves between the screen and the charge, so approval names the
-- price it approves and a changed quote is refused rather than silently
-- charged.
--
-- `registrant` is the personal data a registry requires by contract: a name,
-- an address, a telephone. It rests here, in the tenant's own row, and is
-- read only by the deliberate call that needs it — never by the list, never
-- by a log line. It is JSONB rather than eight columns because nothing may
-- query across it; it is carried to the registrar and nowhere else.
--
-- Billing stays behind its own door. `payment_reference` is an opaque string
-- minted by whatever charges the tenant; Sites never writes a billing table
-- and never reads one. The unique index on it is the other half of that
-- promise: one payment settles exactly one purchase.

CREATE TABLE site_domain_purchases (
    tenant_id              TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    site_id                TEXT NOT NULL,
    id                     TEXT NOT NULL,
    -- 'registration' buys a name; 'renewal' extends one we already hold.
    kind                   TEXT NOT NULL,
    domain                 TEXT NOT NULL,
    tld                    TEXT NOT NULL,
    state                  TEXT NOT NULL DEFAULT 'quoted',
    term_years             INTEGER NOT NULL,
    currency               TEXT NOT NULL,
    first_term_cents       BIGINT NOT NULL,
    renewal_cents_per_year BIGINT NOT NULL,
    premium                BOOLEAN NOT NULL DEFAULT FALSE,
    auto_renew             BOOLEAN NOT NULL DEFAULT TRUE,
    -- The nameservers the registration will be created with, in order.
    nameservers            JSONB NOT NULL,
    -- The registry's required owner record. Personal data; read deliberately.
    registrant             JSONB NOT NULL,
    -- The caller's replay token for *creating* this purchase: a double-clicked
    -- buy button reaches the same row rather than quoting a second domain.
    request_key            TEXT NOT NULL,
    -- Everything a registry would act on, as `DomainOrder::fingerprint` writes
    -- it. A replay of `request_key` that disagrees here is a different purchase
    -- wearing the same token, and is refused.
    order_fingerprint      TEXT NOT NULL,
    -- Who approved this exact price, and when. No approval, no charge.
    approved_at            TIMESTAMPTZ,
    approved_by            TEXT,
    -- Billing's own opaque identifier for the charge. Never parsed here.
    payment_reference      TEXT,
    paid_at                TIMESTAMPTZ,
    -- When the registration sweep last took this row, and how often it has.
    -- The registrar call is idempotent under `id`, so a claim that dies is
    -- retried rather than dropped: the money already moved.
    claimed_at             TIMESTAMPTZ,
    attempts               INTEGER NOT NULL DEFAULT 0,
    -- What the registrar answered.
    provider_reference     TEXT,
    registered_at          TIMESTAMPTZ,
    expires_at             TIMESTAMPTZ,
    lifecycle              TEXT,
    -- When the bought name was attached to the site and made live.
    configured_at          TIMESTAMPTZ,
    -- A safe sentence about why this purchase stopped. Never registrant data.
    failure                TEXT,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites(tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT site_domain_purchases_kind_known
        CHECK (kind IN ('registration', 'renewal')),
    CONSTRAINT site_domain_purchases_state_known
        CHECK (state IN ('quoted', 'approved', 'awaiting_payment', 'paid',
                         'registering', 'registered', 'configured',
                         'failed', 'cancelled')),
    CONSTRAINT site_domain_purchases_term_sane
        CHECK (term_years BETWEEN 1 AND 10),
    CONSTRAINT site_domain_purchases_money_positive
        CHECK (first_term_cents > 0 AND renewal_cents_per_year > 0),
    CONSTRAINT site_domain_purchases_attempts_not_negative
        CHECK (attempts >= 0)
);

-- A replayed create reaches the row it already made.
CREATE UNIQUE INDEX site_domain_purchases_request_key
    ON site_domain_purchases (tenant_id, request_key);

-- One live purchase per name: a tenant cannot buy the same domain twice, and
-- cannot start a second renewal while one is still in flight. A purchase that
-- failed or was called off releases the name for another attempt.
CREATE UNIQUE INDEX site_domain_purchases_one_registration
    ON site_domain_purchases (tenant_id, domain)
    WHERE kind = 'registration' AND state NOT IN ('failed', 'cancelled');

CREATE UNIQUE INDEX site_domain_purchases_one_open_renewal
    ON site_domain_purchases (tenant_id, domain)
    WHERE kind = 'renewal'
      AND state IN ('quoted', 'approved', 'awaiting_payment', 'paid',
                    'registering');

-- One payment settles one purchase.
CREATE UNIQUE INDEX site_domain_purchases_one_payment
    ON site_domain_purchases (tenant_id, payment_reference)
    WHERE payment_reference IS NOT NULL;

CREATE INDEX site_domain_purchases_by_site
    ON site_domain_purchases (tenant_id, site_id, created_at, id);

-- The registration sweep's claim index: paid work waiting, plus claims old
-- enough to have died with their process.
CREATE INDEX site_domain_purchases_registration_queue
    ON site_domain_purchases (created_at, id)
    WHERE state IN ('paid', 'registering');
