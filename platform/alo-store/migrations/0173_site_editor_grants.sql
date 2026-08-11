-- alo Sites S2.03a: a deliberately narrow collaborator. The tenant role is
-- the global signal that closes every non-Sites API door; this table is the
-- resource boundary that opens only the named sites.

ALTER TABLE tenant_user_roles
    DROP CONSTRAINT tenant_user_roles_known;

ALTER TABLE tenant_user_roles
    ADD CONSTRAINT tenant_user_roles_known
        CHECK (role IN ('accountant', 'hr', 'site_editor'));

CREATE TABLE site_editor_grants (
    tenant_id  TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    site_id    TEXT NOT NULL,
    user_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    granted_by TEXT NOT NULL,
    granted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, site_id, user_id),
    FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites (tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX site_editor_grants_by_user
    ON site_editor_grants (tenant_id, user_id, site_id);

-- A site deletion cascades its grants. The last such deletion must also remove
-- the restricted role or the collaborator would be left unable to use either
-- Sites or the ordinary workspace, with no remaining site through which an
-- owner could revoke them.
CREATE FUNCTION cleanup_site_editor_role() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    DELETE FROM tenant_user_roles r
     WHERE r.tenant_id = OLD.tenant_id
       AND r.user_id = OLD.user_id
       AND r.role = 'site_editor'
       AND NOT EXISTS (
           SELECT 1 FROM site_editor_grants g
            WHERE g.tenant_id = OLD.tenant_id
              AND g.user_id = OLD.user_id
       );
    RETURN OLD;
END;
$$;

CREATE TRIGGER site_editor_grants_cleanup_role
AFTER DELETE ON site_editor_grants
FOR EACH ROW EXECUTE FUNCTION cleanup_site_editor_role();
