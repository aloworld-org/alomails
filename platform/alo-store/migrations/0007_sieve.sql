-- Sieve filtering: per-account scripts, vacation per-correspondent
-- suppression, and a per-account redirect rate budget. All rows are
-- account-scoped (tenant_id, user_id) so isolation is inherited.

-- User Sieve scripts. At most one is active per account (the one run at
-- delivery). Content is validated (compiled) before it can be stored.
CREATE TABLE sieve_scripts (
    id         TEXT PRIMARY KEY,
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    content    TEXT NOT NULL,
    active     BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX sieve_scripts_name ON sieve_scripts(tenant_id, user_id, name);
-- At most one active script per account.
CREATE UNIQUE INDEX sieve_scripts_active
    ON sieve_scripts(tenant_id, user_id) WHERE active;

-- Vacation auto-reply suppression: at most one reply per (account, handle,
-- correspondent) per :days window. `handle` scopes suppression (RFC 5230).
CREATE TABLE vacation_responses (
    tenant_id     TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    handle        TEXT NOT NULL,
    correspondent TEXT NOT NULL,
    last_sent     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, user_id, handle, correspondent)
);

-- Per-account redirect rate budget (a rolling window). A compromised or
-- runaway script cannot turn the account into a relay.
CREATE TABLE redirect_budget (
    tenant_id    TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    window_start TIMESTAMPTZ NOT NULL DEFAULT now(),
    count        BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (tenant_id, user_id)
);
