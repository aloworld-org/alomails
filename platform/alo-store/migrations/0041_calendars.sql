-- First-class calendars (Agenda: team/shared calendars, slice 1 — the
-- foundation). Until now each user had one implicit calendar; events were
-- scoped to (tenant, user). Now a user can have several named calendars, and
-- every event belongs to one via `calendar_id`. Sharing a calendar with other
-- people or groups (calendar_grants) is the next slice and is additive to this.
--
-- Access is unchanged for now: a user still sees only their own calendars'
-- events (the store keeps querying by (tenant, user)); calendars just group and
-- colour them. Cross-user visibility arrives with grants.

CREATE TABLE calendars (
    tenant_id     TEXT NOT NULL,
    -- Opaque calendar id (also the CalDAV collection name).
    id            TEXT NOT NULL,
    -- The user who owns/created the calendar. A personal calendar is owned by
    -- its user; a shared calendar too, with grants to others (next slice).
    owner_user_id TEXT NOT NULL,
    name          TEXT NOT NULL,
    -- Optional display colour (hex like #e76f51), for the UI + phone clients.
    color         TEXT,
    -- `personal` (the auto-created default, not deletable) or `shared`.
    kind          TEXT NOT NULL DEFAULT 'personal',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id)
);
CREATE INDEX calendars_by_owner ON calendars (tenant_id, owner_user_id);

-- Give every user who already has events a personal calendar, and move their
-- events onto it. Deterministic id so re-runs are idempotent and the CalDAV
-- href stays stable.
INSERT INTO calendars (tenant_id, id, owner_user_id, name, kind)
SELECT DISTINCT tenant_id, 'cal_personal_' || user_id, user_id, 'Personal', 'personal'
FROM calendar_events
ON CONFLICT (tenant_id, id) DO NOTHING;

ALTER TABLE calendar_events ADD COLUMN calendar_id TEXT;
UPDATE calendar_events SET calendar_id = 'cal_personal_' || user_id WHERE calendar_id IS NULL;
ALTER TABLE calendar_events ALTER COLUMN calendar_id SET NOT NULL;

CREATE INDEX calendar_events_by_calendar
    ON calendar_events (tenant_id, calendar_id, starts_at, ends_at);
