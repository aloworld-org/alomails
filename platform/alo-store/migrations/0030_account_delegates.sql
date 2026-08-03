-- Mailbox delegation (ADR 0017): a grant that lets one user (the delegate)
-- access another user's mailbox (the owner) within the same tenant — the model
-- behind shared mailboxes and "manage someone's inbox". A grant gives the
-- delegate full read + write on the owner's mail; `can_send` additionally
-- permits sending as the owner's address.
--
-- Tenant-scoped by construction: both users belong to `tenant_id`, so a grant
-- can never span tenants. Authorization always looks up grants only within the
-- delegate's own tenant (taken from their token), so a delegate in one tenant
-- can never reach a mailbox in another.
CREATE TABLE account_delegates (
    tenant_id   TEXT    NOT NULL,
    owner_id    TEXT    NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    delegate_id TEXT    NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    can_send    BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (owner_id, delegate_id),
    -- A user is never their own delegate.
    CONSTRAINT account_delegates_distinct CHECK (owner_id <> delegate_id)
);

-- Authorization lookup (owner + delegate) is the PK; these serve the listings.
CREATE INDEX account_delegates_delegate ON account_delegates (tenant_id, delegate_id);
CREATE INDEX account_delegates_owner ON account_delegates (tenant_id, owner_id);
