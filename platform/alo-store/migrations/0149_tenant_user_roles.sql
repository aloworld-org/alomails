-- alo Finance (ADR 0035, wave B4.12): alo's first scoped role
-- (docs/design/finance.md, "The accountant role").
--
-- WHY A ROLE AND NOT A SPACE. A Space is a container with members, and the
-- ledger is not in a container — it IS the tenant. Modelling "who may see the
-- books" as Space membership would answer it with the same table that answers
-- "who may see this folder", and the first tenant who tidied an accountant out
-- of a sidebar would silently revoke their access to the year-end. So the role
-- is tenant-wide, cross-module, and lives here.
--
-- WHY A TABLE AND NOT A SECOND BOOLEAN ON `users`. `is_admin` is a column
-- because there is exactly one of it and it is read on every request. A role
-- set grows — B6's HR role is the next one named — and a column per role is a
-- schema migration per role plus a `WHERE` clause nobody remembers to widen.
-- Rows also carry WHO granted the role and WHEN, which a boolean cannot, and an
-- external accountant's access is precisely the kind of fact an auditor asks
-- the provenance of.
--
-- NOT AN RBAC ENGINE. There is no permissions table and no resource column:
-- one role ships today, the gates name it in words (`require_finance`), and a
-- permission matrix built for a single caller is a matrix that encodes that
-- caller's accidents. The second role widens this table by a value in the CHECK
-- and the gates by a word.
--
-- TENANCY. `tenant_id` is carried beside `user_id` even though `users.id` is
-- globally unique, so every read is tenant-bound by construction like the rest
-- of the store — and the grant path additionally proves the user is a member of
-- the granting tenant before it writes a row (a foreign user id must never
-- become a role holder in a tenant they do not belong to).
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE tenant_user_roles (
    tenant_id  TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    user_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role       TEXT NOT NULL,
    -- Provenance: who handed this access out, and when. Never cleared — a
    -- revoke deletes the row, and the audit log keeps the history.
    granted_by TEXT NOT NULL,
    granted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, user_id, role),
    -- A word this build does not know is a schema disagreement, and a role
    -- nobody gates on is an access fact that silently does nothing.
    CONSTRAINT tenant_user_roles_known
        CHECK (role IN ('accountant'))
);

-- "Who holds this role?" — the admin console's user list asks it once per page,
-- and the finance surfaces will ask it per tenant.
CREATE INDEX tenant_user_roles_by_role
    ON tenant_user_roles (tenant_id, role);
