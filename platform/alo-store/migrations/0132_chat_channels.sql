-- alo Chat (ADR 0038): channels, DMs and their membership. Tenant-scoped and
-- cascading with the tenant like every other table; no cross-tenant surface at
-- all — a chat room is never addressable from outside its tenant.
--
-- Membership IS the permission (docs/design/chat.md): a private channel or a
-- DM is visible only to its members, a public channel to any user of the
-- tenant. Messages and the per-channel sequence land in the next phase; the
-- read cursor they will move (`last_read_seq`) is already here so the column
-- never has to be added under live rows.

CREATE TABLE chat_channels (
    tenant_id   TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    id          TEXT NOT NULL,
    -- 'channel' (a named room) | 'dm' (exactly two people).
    kind        TEXT NOT NULL,
    -- The human label of a named room; a DM has none (it is named by whoever
    -- you are talking to, which the reader resolves).
    name        TEXT,
    topic       TEXT,
    -- 'public' (any user of the tenant may see and join) | 'private'.
    -- A DM is always private.
    visibility  TEXT NOT NULL,
    -- For a DM only: both member ids sorted and joined, so opening the same
    -- conversation twice returns the same room (the API's idempotency).
    dm_key      TEXT,
    created_by  TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Archived rooms stay readable; they leave the lists and free their name.
    archived_at TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, id),
    CONSTRAINT chat_channels_kind CHECK (kind IN ('channel', 'dm')),
    CONSTRAINT chat_channels_visibility CHECK (visibility IN ('public', 'private')),
    -- The two shapes are exclusive: a named room has a name and no dm_key; a
    -- DM has a dm_key, no name, and is never public.
    CONSTRAINT chat_channels_shape CHECK (
        (kind = 'channel' AND name IS NOT NULL AND dm_key IS NULL)
        OR (kind = 'dm' AND name IS NULL AND dm_key IS NOT NULL AND visibility = 'private')
    )
);

-- One DM per pair per tenant — the idempotency the API promises, enforced by
-- the database so two simultaneous opens cannot make two rooms.
CREATE UNIQUE INDEX chat_channels_dm_key
    ON chat_channels (tenant_id, dm_key)
    WHERE dm_key IS NOT NULL;

-- One live `#name` per tenant (the Slack reflex). Archiving frees the name.
CREATE UNIQUE INDEX chat_channels_name
    ON chat_channels (tenant_id, lower(name))
    WHERE kind = 'channel' AND archived_at IS NULL;

CREATE TABLE chat_members (
    tenant_id     TEXT NOT NULL,
    channel_id    TEXT NOT NULL,
    user_id       TEXT NOT NULL,
    -- 'owner' (may rename, archive, remove others) | 'member'.
    role          TEXT NOT NULL DEFAULT 'member',
    joined_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- The read cursor: the last per-channel sequence this person has seen.
    -- 0 = nothing read. Sequences arrive with messages in the next phase.
    last_read_seq BIGINT NOT NULL DEFAULT 0,
    muted         BOOLEAN NOT NULL DEFAULT false,
    PRIMARY KEY (tenant_id, channel_id, user_id),
    FOREIGN KEY (tenant_id, channel_id)
        REFERENCES chat_channels (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT chat_members_role CHECK (role IN ('owner', 'member'))
);

-- "Which rooms am I in?" — the query every chat screen opens with.
CREATE INDEX chat_members_by_user ON chat_members (tenant_id, user_id);
