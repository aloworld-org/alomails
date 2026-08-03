-- Per-user mail settings (the signature) and the tenant-wide organization
-- footer (mail daily-driver). Additive with empty defaults, so no behavior
-- change: an unset signature/footer is the empty string.
CREATE TABLE user_settings (
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id    TEXT NOT NULL,
    signature  TEXT NOT NULL DEFAULT '',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, user_id)
);

ALTER TABLE tenants ADD COLUMN org_footer TEXT NOT NULL DEFAULT '';
