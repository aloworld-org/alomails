-- App-specific passwords (mail M1): per-user, named credentials for
-- legacy clients (IMAP/POP3 LOGIN, SMTP AUTH) that cannot carry a second
-- factor. Generated server-side by alo-identity, stored only as an
-- argon2id PHC hash — the store never holds the secret, and revocation
-- is deletion, so this table only ever lists what can still log in.
CREATE TABLE app_passwords (
    id            TEXT PRIMARY KEY,
    tenant_id     TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at  TIMESTAMPTZ
);
CREATE INDEX app_passwords_user ON app_passwords(tenant_id, user_id);
