-- Large-file share links (Ficina Transfer): a file too big to attach is stored
-- as a blob and sent as a private, expiring download link instead of an inline
-- attachment. The link's token is stored hashed (SHA-256), so a database read
-- never exposes a live link; the public download route hashes the incoming
-- token to look the row up. The referenced blob is reclaimed by the expiry
-- sweeper once no live share points at it.
CREATE TABLE file_shares (
    token_hash   TEXT        PRIMARY KEY,
    tenant_id    TEXT        NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id      TEXT        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    blob_id      TEXT        NOT NULL,
    filename     TEXT        NOT NULL,
    content_type TEXT        NOT NULL,
    size         BIGINT      NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at   TIMESTAMPTZ NOT NULL
);

-- The expiry sweeper scans by due time; the blob index answers "any other live
-- share for this blob?" during reclamation.
CREATE INDEX file_shares_expiry_idx ON file_shares (expires_at);
CREATE INDEX file_shares_blob_idx ON file_shares (tenant_id, blob_id);
