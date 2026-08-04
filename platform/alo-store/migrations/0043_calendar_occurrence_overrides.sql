-- Per-occurrence overrides (Agenda: edit ONE instance of a recurring series in
-- place — "move just this Tuesday's standup to 3pm and rename it", while the
-- rest of the series stays put). This is the iCalendar RECURRENCE-ID model: an
-- override is a detached copy of one occurrence, keyed by the original slot it
-- replaces. Skipping one occurrence (EXDATE) already lives on the master row's
-- `exdates`; this is its editing counterpart.
--
-- Kept in its own table so `calendar_events` stays "masters + one-offs" with an
-- unchanged primary key. A row here overrides the occurrence of `series_id`
-- whose ORIGINAL start is `recurrence_id`; `starts_at`/`ends_at` are that
-- occurrence's NEW time (may differ from `recurrence_id`). Only the fields a
-- user edits per-instance are overridable — attendees and the rule itself are
-- not (an override cannot itself recur). Tenant/user scoped exactly like the
-- events it belongs to.
CREATE TABLE calendar_event_overrides (
    tenant_id     TEXT NOT NULL,
    user_id       TEXT NOT NULL,
    -- The master event id (iCalendar UID) this overrides an occurrence of.
    series_id     TEXT NOT NULL,
    -- The ORIGINAL start of the occurrence being replaced (the RECURRENCE-ID).
    recurrence_id TIMESTAMPTZ NOT NULL,
    summary       TEXT NOT NULL,
    description   TEXT,
    location      TEXT,
    -- The occurrence's NEW bounds (UTC; `ends_at` exclusive, >= `starts_at`).
    starts_at     TIMESTAMPTZ NOT NULL,
    ends_at       TIMESTAMPTZ NOT NULL,
    all_day       BOOLEAN NOT NULL DEFAULT false,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, series_id, recurrence_id)
);

-- "Which overrides land in this window?" — by their NEW start, the read path.
CREATE INDEX calendar_event_overrides_range
    ON calendar_event_overrides (tenant_id, series_id, starts_at);
