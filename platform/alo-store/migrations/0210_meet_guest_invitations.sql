CREATE TABLE IF NOT EXISTS meeting_guest_invitations (
    tenant_id text NOT NULL,
    id text NOT NULL,
    meeting_id text NOT NULL,
    token_hash text NOT NULL UNIQUE CHECK (token_hash ~ '^[0-9a-f]{64}$'),
    guest_email text NOT NULL,
    guest_name text NOT NULL,
    expires_at timestamptz NOT NULL,
    requested_at timestamptz,
    admitted_at timestamptz,
    denied_at timestamptz,
    revoked_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, meeting_id) REFERENCES meetings (tenant_id, id)
);
CREATE INDEX IF NOT EXISTS meeting_guest_lobby
    ON meeting_guest_invitations (tenant_id, meeting_id, requested_at)
    WHERE requested_at IS NOT NULL AND admitted_at IS NULL AND denied_at IS NULL AND revoked_at IS NULL;
