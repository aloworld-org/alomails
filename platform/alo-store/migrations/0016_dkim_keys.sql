-- Per-tenant DKIM signing keys (ADR 0014). One row per (tenant, domain,
-- selector); at most one row per domain is `active` (the key the signer uses).
-- Stores the Ed25519 secret seed and the raw public key; the signer rebuilds
-- the PKCS#8 key from the seed at sign time. The seed is secret — it is never
-- returned to a client. Additive; the existing file/env DKIM key is unaffected
-- and remains the single-tenant / fallback path.
CREATE TABLE dkim_keys (
    id          TEXT PRIMARY KEY,
    tenant_id   TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    domain      TEXT NOT NULL,
    selector    TEXT NOT NULL,
    algorithm   TEXT NOT NULL DEFAULT 'ed25519-sha256',
    seed        BYTEA NOT NULL,
    public_raw  BYTEA NOT NULL,
    active      BOOLEAN NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, domain, selector)
);

-- At most one active signing key per domain (a domain belongs to one tenant —
-- domains table PK — so this is a global one-active-per-domain guarantee).
CREATE UNIQUE INDEX dkim_keys_one_active_per_domain ON dkim_keys (domain) WHERE active;

-- The signer resolves the active key by the sending domain.
CREATE INDEX dkim_keys_tenant_domain ON dkim_keys (tenant_id, domain);
