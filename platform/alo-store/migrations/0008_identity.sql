-- ficina-identity v1: aliases, groups, TOTP 2FA, and the OAuth/OIDC
-- provider. The interim `api_tokens` table is retired in favour of
-- revocable `access_tokens` + `refresh_tokens`. The `credentials` table
-- (0003) stays as the argon2 password-hash store; hashing/verification
-- moves to ficina-identity (the store only persists the PHC string).

-- Additional inbound addresses that route to a canonical user. Globally
-- unique (lowercased at write) so account_by_email routing is
-- deterministic and never guesses between two accounts.
CREATE TABLE aliases (
    address    TEXT PRIMARY KEY,
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX aliases_user ON aliases(tenant_id, user_id);

-- Named membership sets within a tenant. The model ships now; group-based
-- authorization (send-as, shared mailboxes) is wired later.
CREATE TABLE groups (
    id         TEXT PRIMARY KEY,
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, name)
);
CREATE TABLE group_members (
    group_id  TEXT NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id   TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    PRIMARY KEY (group_id, user_id)
);
CREATE INDEX group_members_user ON group_members(tenant_id, user_id);

-- TOTP (RFC 6238) enrollment. `secret` is the raw shared secret. `enabled`
-- flips true only after the user confirms a code, so an un-confirmed
-- secret never gates a login and a half-finished enrollment cannot lock a
-- user out.
CREATE TABLE totp_secrets (
    user_id    TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    secret     BYTEA NOT NULL,
    enabled    BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Single-use recovery codes (SHA-256 hash at rest; constant-time compare).
CREATE TABLE recovery_codes (
    id         TEXT PRIMARY KEY,
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash  TEXT NOT NULL,
    used_at    TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX recovery_codes_user ON recovery_codes(tenant_id, user_id);

-- OAuth clients. Phase 1 registers first-party public clients (PKCE, no
-- secret): secret_hash NULL = public. A NULL tenant_id is a
-- deployment-wide first-party client (the web app) usable by every
-- tenant's users; the tenant comes from the authenticated user, never the
-- client. redirect_uris are exact-matched.
CREATE TABLE oauth_clients (
    client_id     TEXT PRIMARY KEY,
    tenant_id     TEXT REFERENCES tenants(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    redirect_uris TEXT[] NOT NULL,
    secret_hash   TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Authorization codes: hashed, single-use, short-lived, carrying the PKCE
-- challenge and the OIDC nonce captured at /authorize.
CREATE TABLE oauth_auth_codes (
    code_hash      TEXT PRIMARY KEY,
    tenant_id      TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id        TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    client_id      TEXT NOT NULL REFERENCES oauth_clients(client_id) ON DELETE CASCADE,
    redirect_uri   TEXT NOT NULL,
    code_challenge TEXT NOT NULL,
    scope          TEXT NOT NULL,
    nonce          TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at     TIMESTAMPTZ NOT NULL,
    used_at        TIMESTAMPTZ
);

-- Access tokens: opaque, SHA-256 hashed at rest, revocable. Replaces the
-- interim api_tokens.
CREATE TABLE access_tokens (
    token_hash TEXT PRIMARY KEY,
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    client_id  TEXT REFERENCES oauth_clients(client_id) ON DELETE SET NULL,
    scope      TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ
);
CREATE INDEX access_tokens_user ON access_tokens(tenant_id, user_id);

-- Refresh tokens: opaque, hashed, rotated on use. `rotated_to` chains a
-- rotation so that reuse of a spent token is detectable (replay defense).
CREATE TABLE refresh_tokens (
    token_hash TEXT PRIMARY KEY,
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    client_id  TEXT NOT NULL REFERENCES oauth_clients(client_id) ON DELETE CASCADE,
    scope      TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    rotated_to TEXT
);
CREATE INDEX refresh_tokens_user ON refresh_tokens(tenant_id, user_id);

-- ID-token signing keys (deployment-global). Ed25519 (EdDSA, RFC 8037).
-- The newest non-retired key signs; every non-retired key's public half
-- is published in the JWKS. Rotation inserts a new key and retires old
-- ones after a grace window.
CREATE TABLE signing_keys (
    kid         TEXT PRIMARY KEY,
    algorithm   TEXT NOT NULL,
    private_key BYTEA NOT NULL,
    public_key  BYTEA NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    retired_at  TIMESTAMPTZ
);

-- Retire the interim bearer-token table; access_tokens supersedes it.
DROP TABLE api_tokens;
