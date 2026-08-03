-- Tenant-admin authorization. A single boolean marks a user as a tenant admin;
-- admin-only surfaces (the admin console, AI-provider configuration) gate on it.
-- The bootstrap admin (identityctl bootstrap-admin) is set admin at creation.
-- Existing users default to non-admin — an operator promotes their admin with:
--   UPDATE users SET is_admin = TRUE WHERE tenant_id = '...' AND email = '...';
ALTER TABLE users ADD COLUMN is_admin BOOLEAN NOT NULL DEFAULT FALSE;
