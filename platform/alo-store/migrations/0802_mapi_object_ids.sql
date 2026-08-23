-- Durable MAPI identifiers for folders and messages (ADR 0051, stage 8).
--
-- WHY THIS EXISTS AT ALL. A MAPI client addresses a folder or message by an
-- 8-byte id whose lower 48 bits are a counter. Until now alo derived that
-- counter by hashing the store's own opaque id (FNV-1a, folded into 48 bits).
-- That was a reasonable choice while a client could only *read*: the server
-- always held the folder's message list in memory, so it matched an incoming
-- id by scanning, and a hash that is merely stable was enough.
--
-- Incremental synchronization changes the requirement. A cached-mode client
-- keeps its own replica for years and hands back sets of ids it holds; the
-- server answers with what changed outside that set. Two properties become
-- load-bearing that a hash cannot promise:
--
--   1. UNIQUENESS. 48 bits over a large mailbox collides by the birthday bound
--      long before the space is exhausted. Two messages sharing an id is not a
--      visible failure — the client simply never sees the second one, forever,
--      and no error is raised anywhere. That is silent mail loss.
--   2. PERMANENCE. An id the client was given must never be reissued to a
--      different message, including after the original is deleted. A hash of a
--      recycled store id offers no such guarantee.
--
-- So the id becomes a fact we record rather than a number we recompute.
--
-- WHY ONE COUNTER SPACE PER ACCOUNT, not one per object kind. Exchange
-- allocates folder and message ids from a single per-replica counter, and an
-- account is alo's replica. Keeping one space means a folder id and a message
-- id can never coincide, so a set of ids is unambiguous even where the two
-- kinds are compared — which is exactly the sort of assumption that is cheap
-- now and impossible to add later.
--
-- WHY THE COUNTER IS ITS OWN TABLE. Allocating with
-- `SELECT max(counter) + 1` would scan, and would race: two concurrent
-- deliveries would read the same maximum and one would lose. A single row
-- updated with `... DO UPDATE SET next_counter = next_counter + 1 RETURNING`
-- is atomic under Postgres' row lock, costs one indexed write, and cannot
-- hand the same number to two callers.
--
-- WHY ids START ABOVE A RESERVED BAND. The special folders (the mailbox root,
-- Inbox, and the rest) hold fixed low counters that a client expects to find
-- at known values. Allocation begins above them so an ordinary folder can
-- never take a special folder's id.
--
-- Expand-only: two new tables, no column added to and no constraint placed on
-- anything that already holds rows. Existing hashed ids keep working for the
-- read paths until those are migrated to read from here.

-- The counter allocator for one account. One row per mailbox.
CREATE TABLE mapi_id_counter (
    tenant_id    TEXT   NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    user_id      TEXT   NOT NULL REFERENCES users (id)   ON DELETE CASCADE,
    -- The highest counter handed out so far, so that
    -- `... DO UPDATE SET last_counter = last_counter + 1 RETURNING last_counter`
    -- both advances and yields the allocation in one atomic statement. The
    -- default sits just below the first allocatable value: the allocator
    -- inserts 1024 explicitly on the first call for an account, and every call
    -- after that increments.
    last_counter BIGINT NOT NULL DEFAULT 1023,
    PRIMARY KEY (tenant_id, user_id),
    -- A counter is 48 bits. Crossing that ceiling would silently truncate an
    -- id and alias two objects, so it is refused at the database instead.
    CONSTRAINT mapi_id_counter_fits_48_bits
        CHECK (last_counter > 0 AND last_counter < 281474976710656)
);

COMMENT ON TABLE mapi_id_counter IS
    'Per-account allocator for MAPI folder and message ids (ADR 0051).';

-- The identifier a MAPI client knows an object by, and what it maps to.
CREATE TABLE mapi_object_id (
    tenant_id  TEXT        NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    user_id    TEXT        NOT NULL REFERENCES users (id)   ON DELETE CASCADE,
    -- Which kind of object the store id names. Both share one counter space,
    -- but the kind is still recorded: it says which table `store_id` points
    -- into, and a lookup that ignored it could return a folder for a message.
    kind       TEXT        NOT NULL,
    -- The store's own opaque id (a mailbox id or a message id).
    store_id   TEXT        NOT NULL,
    -- The 48-bit counter half of the id the client holds.
    counter    BIGINT      NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    PRIMARY KEY (tenant_id, user_id, kind, store_id),

    CONSTRAINT mapi_object_id_kind_known
        CHECK (kind IN ('folder', 'message')),
    CONSTRAINT mapi_object_id_fits_48_bits
        CHECK (counter > 0 AND counter < 281474976710656)
);

-- The promise the whole design rests on: within one account, a counter names
-- exactly one object. Deliberately spans both kinds — see the header.
CREATE UNIQUE INDEX mapi_object_id_counter_unique
    ON mapi_object_id (tenant_id, user_id, counter);

-- Resolving an id a client sent back is the hot path for synchronization, and
-- it arrives as a counter rather than a store id.
CREATE INDEX mapi_object_id_by_counter
    ON mapi_object_id (tenant_id, user_id, kind, counter);

COMMENT ON TABLE mapi_object_id IS
    'Stable MAPI ids: what a client calls a folder or message (ADR 0051).';
