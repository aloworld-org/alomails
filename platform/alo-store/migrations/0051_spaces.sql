-- Spaces: the membership spine of the workspace (ADR 0026). A Space is a
-- tenant-scoped group with explicit members and per-member roles; modules
-- (files first, tasks/mailbox/feed later) attach to it and inherit its
-- membership. Personal "My Files" is NOT a space — it is a user's own location.
-- Everything cascades with the tenant (Law 1).

CREATE TABLE spaces (
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    id         TEXT NOT NULL,
    name       TEXT NOT NULL,
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    archived   BOOLEAN NOT NULL DEFAULT false,
    PRIMARY KEY (tenant_id, id)
);

-- One row per (space, user); role is the whole permission model.
--   viewer  — read the space's contents
--   editor  — viewer + create/edit/upload/move/delete within the space
--   manager — editor + membership, rename/archive, enable/disable modules
CREATE TABLE space_members (
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    space_id  TEXT NOT NULL,
    user_id   TEXT NOT NULL,
    role      TEXT NOT NULL,
    added_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, space_id, user_id)
);
-- "Which spaces am I in?" — the hot path for every listing.
CREATE INDEX space_members_by_user ON space_members (tenant_id, user_id);

-- Which modules are enabled on a space (additive: a new module needs no schema
-- change to the space itself). 'files' is enabled on create.
CREATE TABLE space_modules (
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    space_id  TEXT NOT NULL,
    module    TEXT NOT NULL,
    PRIMARY KEY (tenant_id, space_id, module)
);
