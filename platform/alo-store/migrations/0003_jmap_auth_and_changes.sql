-- Interim bearer auth (until ficina-identity) and JMAP change tracking.

-- Blobs carry a Content-Type for JMAP download (served verbatim, no
-- sniffing). Ingested message blobs are message/rfc822; uploaded blobs
-- carry the client's declared type.
ALTER TABLE blobs ADD COLUMN content_type TEXT;

-- Login credentials (argon2 PHC hash). Username is the global login key
-- so the token endpoint can resolve it without a tenant hint.
CREATE TABLE credentials (
    user_id       TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    tenant_id     TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    username      TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX credentials_username ON credentials(username);

-- Issued bearer tokens, stored only as a SHA-256 hash of the token.
CREATE TABLE api_tokens (
    token_hash TEXT PRIMARY KEY,
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ
);
CREATE INDEX api_tokens_tenant ON api_tokens(tenant_id);

-- Per-tenant monotonic modseq — the JMAP state cursor.
CREATE TABLE tenant_modseq (
    tenant_id TEXT PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    modseq    BIGINT NOT NULL DEFAULT 0
);

-- One row per object, upserted on every create/update/destroy to the
-- new modseq. Answers /changes (created/updated/destroyed since S) in a
-- single indexed range scan on (tenant_id, type, modseq). A destroyed
-- object keeps a tombstone (destroyed = true).
CREATE TABLE object_changes (
    tenant_id      TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    type           TEXT NOT NULL,
    id             TEXT NOT NULL,
    created_modseq BIGINT NOT NULL,
    modseq         BIGINT NOT NULL,
    destroyed      BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (tenant_id, type, id)
);
CREATE INDEX object_changes_scan ON object_changes(tenant_id, type, modseq);
