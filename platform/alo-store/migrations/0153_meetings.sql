-- A meeting: the record alo owns, beside the media it does not.
--
-- LiveKit runs the audio and video as a sealed container and knows nothing
-- about tenants, calendars or rooms. It only ever sees an opaque room name and
-- a signed token. Everything that makes a meeting *ours* — who may join, what
-- it belongs to, who was in it — lives here, so the engine stays swappable and
-- no tenant fact ever crosses into it.
--
-- `room` is what LiveKit is told. It is generated, never derived from a title:
-- a room name built from "Q3 Budget — Acme" would leak a customer's name to
-- the engine and into its logs.
CREATE TABLE IF NOT EXISTS meetings (
    tenant_id   text        NOT NULL,
    id          text        NOT NULL,
    -- The opaque name the media engine knows this by. Unique across the
    -- deployment because the engine has no tenants of its own.
    room        text        NOT NULL,
    title       text        NOT NULL DEFAULT '',
    created_by  text        NOT NULL,
    -- What it belongs to, if anything: a chat room, a calendar event, or
    -- neither for a meeting somebody started from nowhere.
    channel_id  text,
    event_id    text,
    created_at  timestamptz NOT NULL DEFAULT now(),
    -- When somebody first joined, and when it was declared over. A meeting
    -- nobody joined is not a meeting that happened.
    started_at  timestamptz,
    ended_at    timestamptz,
    PRIMARY KEY (tenant_id, id)
);

CREATE UNIQUE INDEX IF NOT EXISTS meetings_room_key ON meetings (room);
CREATE INDEX IF NOT EXISTS meetings_by_channel ON meetings (tenant_id, channel_id)
    WHERE channel_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS meetings_live ON meetings (tenant_id, ended_at)
    WHERE ended_at IS NULL;

-- Who was in it, and when. Written when somebody takes a token, so the record
-- of attendance is ours rather than something we have to ask the engine for
-- later — engines are swappable, attendance is evidence.
CREATE TABLE IF NOT EXISTS meeting_participants (
    tenant_id  text        NOT NULL,
    meeting_id text        NOT NULL,
    user_id    text        NOT NULL,
    joined_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, meeting_id, user_id)
);
