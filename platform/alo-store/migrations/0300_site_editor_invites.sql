-- alo Sites S2.03b: one-time setup links for deliberately restricted site
-- collaborators. The raw token exists only in the URL shown to the inviter;
-- the database keeps its SHA-256 hash. An accepted row is retained so the
-- account can be recognised as invite-created and removed when its final site
-- grant is revoked.

CREATE TABLE site_editor_invites (
    token_hash  TEXT PRIMARY KEY,
    tenant_id   TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    user_id     TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    email       TEXT NOT NULL,
    invited_by  TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ NOT NULL,
    accepted_at TIMESTAMPTZ,
    UNIQUE (tenant_id, user_id)
);

CREATE INDEX site_editor_invites_by_tenant
    ON site_editor_invites (tenant_id, user_id);
