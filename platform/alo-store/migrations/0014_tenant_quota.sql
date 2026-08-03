-- Per-tenant storage quota (ADR 0012). NULL = unlimited (the default, so this
-- is inert until an operator sets a cap — no behavior change on existing
-- deployments). Enforced at the blob-write choke points (message ingest and
-- blob upload); only genuinely new bytes count, since blobs are deduplicated
-- per tenant.
ALTER TABLE tenants ADD COLUMN storage_quota_bytes BIGINT;
