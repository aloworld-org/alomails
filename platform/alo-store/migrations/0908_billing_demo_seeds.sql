-- Version ledger for the explicit local-development Billing corpus. The row is
-- written in the same transaction as the corpus, making repeated seed calls a
-- read rather than another insert. Production cannot reach the store service
-- without first passing its runtime environment/database guard.
CREATE TABLE billing_demo_seeds (
    tenant_id   TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    version     INTEGER NOT NULL,
    seeded_by   TEXT NOT NULL,
    settings_inserted BOOLEAN NOT NULL DEFAULT false,
    fx_currencies TEXT[] NOT NULL DEFAULT '{}',
    seeded_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id),
    CONSTRAINT billing_demo_seeds_version_positive CHECK (version > 0)
);
