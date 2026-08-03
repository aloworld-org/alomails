-- Per-account change tracking: JMAP accountId is the user, so /changes
-- must be scoped to the account, not just the tenant. Record the owning
-- user on each change and index for the per-account range scan.
ALTER TABLE object_changes ADD COLUMN user_id TEXT;

DROP INDEX IF EXISTS object_changes_scan;
CREATE INDEX object_changes_scan
    ON object_changes(tenant_id, user_id, type, modseq);
