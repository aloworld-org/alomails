-- Calendar (Agenda) — slice 1: a single implicit calendar per user, plain
-- timed or all-day events. Tenant/user scoped exactly like contacts (0034).
-- Recurrence, attendees, multiple calendars, and CalDAV sharing come in later
-- slices and are additive to this table.
CREATE TABLE calendar_events (
    tenant_id   TEXT NOT NULL,
    user_id     TEXT NOT NULL,
    -- Opaque event id = the iCalendar UID (stable across a future CalDAV sync).
    id          TEXT NOT NULL,
    -- iCalendar SUMMARY (the title). Never empty (enforced by the caller).
    summary     TEXT NOT NULL,
    description TEXT,
    location    TEXT,
    -- UTC instants. An all-day event uses midnight-UTC bounds; the client
    -- renders it date-only. `ends_at` is exclusive and >= `starts_at`.
    starts_at   TIMESTAMPTZ NOT NULL,
    ends_at     TIMESTAMPTZ NOT NULL,
    all_day     BOOLEAN NOT NULL DEFAULT false,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, user_id, id)
);

-- Range queries ("events overlapping this month/week") are the hot path.
CREATE INDEX calendar_events_range
    ON calendar_events (tenant_id, user_id, starts_at, ends_at);
