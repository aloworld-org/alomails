-- alo Sites S2.06a: pages the internet can reach but only with a password.
--
-- Protection is deliberately NOT frozen into `site_page_snapshots` with the
-- rest of a publish. A password is a security decision about the page that is
-- online *now*: setting one, changing it, or lifting it has to take effect on
-- the next request, not at the next publish. So it lives beside the site,
-- keyed by the page identity the snapshots also carry (`page_id`), and the
-- public gate reads it live.
--
-- The foreign key points at `sites`, not at `site_pages`, for the same reason:
-- deleting the draft page does not unpublish its snapshot, so letting the
-- protection cascade away with the draft would silently open a page that is
-- still being served. Protection ends when somebody removes it, or when the
-- whole site goes.

CREATE TABLE site_page_passwords (
    tenant_id     TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    site_id       TEXT NOT NULL,
    -- The page identity shared by the draft page and every snapshot of it,
    -- across locales: one password protects a page in all its languages.
    page_id       TEXT NOT NULL,
    -- argon2id PHC string. The plaintext is never stored, never logged, and
    -- never returned by any read.
    password_hash TEXT NOT NULL,
    -- An opaque token derived from the stored hash. The public service mints
    -- visitor sessions against it instead of against the hash, so changing or
    -- removing the password ends every session that was opened with the old
    -- one — and the hash itself never leaves the store.
    version       TEXT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, site_id, page_id),
    FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites (tenant_id, id) ON DELETE CASCADE
);
