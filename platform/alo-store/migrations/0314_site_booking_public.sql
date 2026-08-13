-- The public half of alo Sites bookings: what a published page offers, and
-- what a visitor reserved on it.
--
-- `site_booking_snapshots` is the booking service frozen into one publish,
-- exactly as `site_catalog_snapshots` freezes a catalog. A published page must
-- offer the appointment length, the week and the questions that were true when
-- the owner pressed publish, not the ones they are editing this afternoon. The
-- calendar id travels with the snapshot because availability is read against
-- it: the frozen row is the only thing the anonymous service ever resolves.
--
-- `site_booking_appointments` is the reservation ledger, and it is what makes
-- double-booking impossible rather than unlikely: the partial unique index
-- below is the race the design has to win — two visitors pressing *book* on the
-- same free slot in the same instant. The second insert fails on the index, and
-- the visitor is told the time has just been taken. The Agenda event written
-- afterwards is the owner's view of the same fact, not the fact itself.
--
-- Privacy: an appointment stores what the visitor typed (their name, an address
-- to confirm to, and the answers the owner asked for) and nothing about their
-- connection — no IP, no user agent, no referrer. `notified_at` is the
-- at-most-once claim marker the owner-notification sweep will use.

CREATE TABLE site_booking_snapshots (
    tenant_id        TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    publish_id       TEXT NOT NULL,
    booking_id       TEXT NOT NULL,
    site_id          TEXT NOT NULL,
    name             TEXT NOT NULL,
    description      TEXT,
    calendar_id      TEXT NOT NULL,
    time_zone        TEXT NOT NULL,
    duration_minutes INTEGER NOT NULL,
    buffer_minutes   INTEGER NOT NULL,
    notice_minutes   INTEGER NOT NULL,
    horizon_days     INTEGER NOT NULL,
    location         TEXT,
    hours            JSONB NOT NULL,
    fields           JSONB NOT NULL,
    -- Frozen too: a service switched off before the publish shows on the page
    -- as not taking bookings, rather than silently vanishing from it.
    active           BOOLEAN NOT NULL,
    PRIMARY KEY (tenant_id, publish_id, booking_id),
    FOREIGN KEY (tenant_id, publish_id)
        REFERENCES site_publishes(tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX site_booking_snapshots_by_publish
    ON site_booking_snapshots (tenant_id, publish_id, booking_id);

-- The public resolver reaches a service by its bare id (the page carries
-- nothing else), so that lookup gets its own index.
CREATE INDEX site_booking_snapshots_by_booking
    ON site_booking_snapshots (booking_id);

CREATE TABLE site_booking_appointments (
    tenant_id     TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    site_id       TEXT NOT NULL,
    id            TEXT NOT NULL,
    booking_id    TEXT NOT NULL,
    -- The service's name as it was published, so a renamed service does not
    -- rewrite what the visitor believes they booked.
    booking_name  TEXT NOT NULL,
    -- The Agenda calendar the slot was taken from. Kept here so availability
    -- can subtract reservations without depending on the event write having
    -- landed yet.
    calendar_id   TEXT NOT NULL,
    starts_at     TIMESTAMPTZ NOT NULL,
    ends_at       TIMESTAMPTZ NOT NULL,
    -- IANA zone the published week was written in, for showing the visitor
    -- their appointment in the same clock the owner offered it in.
    time_zone     TEXT NOT NULL,
    visitor_name  TEXT NOT NULL,
    visitor_email TEXT NOT NULL,
    -- Answers to the service's own questions, keyed by the frozen field key.
    answers       JSONB NOT NULL,
    -- The Agenda event this appointment produced, once written.
    event_id      TEXT,
    status        TEXT NOT NULL DEFAULT 'booked',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- NULL means nobody has been told yet (the notification sweep's claim).
    notified_at   TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites(tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT site_booking_appointments_span CHECK (ends_at > starts_at),
    CONSTRAINT site_booking_appointments_status
        CHECK (status IN ('booked', 'cancelled'))
);

-- One live appointment per service per instant. This is the race-safety
-- guarantee itself, not a hint: concurrent reservations of one slot are
-- serialized by the index, and exactly one of them commits.
CREATE UNIQUE INDEX site_booking_appointments_one_per_slot
    ON site_booking_appointments (tenant_id, booking_id, starts_at)
    WHERE status = 'booked';

-- Availability subtracts every live appointment on the bound calendar, whatever
-- service took it.
CREATE INDEX site_booking_appointments_by_calendar
    ON site_booking_appointments (tenant_id, calendar_id, starts_at)
    WHERE status = 'booked';

CREATE INDEX site_booking_appointments_by_site
    ON site_booking_appointments (tenant_id, site_id, starts_at, id);
