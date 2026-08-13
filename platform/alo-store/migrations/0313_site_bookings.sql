-- alo Sites booking services: what a visitor may book on a published site —
-- a haircut, a viewing, a thirty-minute call — and the Agenda calendar the
-- appointment will be read against and written into.
--
-- The calendar is named by id and deliberately carries NO foreign key to
-- `calendars`: Agenda owns the lifetime of a calendar, and a booking service
-- must neither block its deletion (RESTRICT) nor disappear with it (CASCADE).
-- The binding is re-resolved through the Sites-owned seam on every read, so a
-- source that has gone away is reported as missing rather than silently
-- serving nothing. Tenancy is enforced the same way every other site-owned
-- row is: (tenant_id, site_id) references `sites`, and every statement scopes
-- by both.
--
-- `hours` is the weekly opening pattern (ISO weekday 1=Monday..7=Sunday plus
-- minute-of-day bounds) and `fields` the extra questions a visitor answers.
-- Both are validated as typed Rust before they are written, never on read
-- alone; the CHECKs below are the last line, not the first.

CREATE TABLE site_bookings (
    tenant_id        TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    site_id          TEXT NOT NULL,
    id               TEXT NOT NULL,
    name             TEXT NOT NULL,
    description      TEXT,
    -- The Agenda calendar this service reads availability from and books into.
    calendar_id      TEXT NOT NULL,
    -- IANA zone the opening hours are written in ("Europe/Brussels").
    time_zone        TEXT NOT NULL,
    duration_minutes INTEGER NOT NULL,
    -- Quiet time kept after each appointment.
    buffer_minutes   INTEGER NOT NULL DEFAULT 0,
    -- Shortest notice a visitor may book at.
    notice_minutes   INTEGER NOT NULL DEFAULT 0,
    -- How far ahead the public calendar opens.
    horizon_days     INTEGER NOT NULL,
    location         TEXT,
    hours            JSONB NOT NULL,
    fields           JSONB NOT NULL,
    -- Off means the service exists but takes no bookings.
    active           BOOLEAN NOT NULL DEFAULT TRUE,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites(tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT site_bookings_duration_positive CHECK (duration_minutes > 0),
    CONSTRAINT site_bookings_buffer_not_negative CHECK (buffer_minutes >= 0),
    CONSTRAINT site_bookings_notice_not_negative CHECK (notice_minutes >= 0),
    CONSTRAINT site_bookings_horizon_positive CHECK (horizon_days > 0)
);

CREATE INDEX site_bookings_by_site
    ON site_bookings (tenant_id, site_id, created_at, id);
