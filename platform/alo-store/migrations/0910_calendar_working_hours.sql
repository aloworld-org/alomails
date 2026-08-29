-- A person's working schedule (Agenda launch tier: working hours and
-- time-zone sanity for cross-border teams). One row per person; no row
-- means the default, Mon–Fri 09:00–17:00, so nobody has to configure
-- anything before scheduling starts distinguishing "busy" from "outside
-- their hours".
--
-- Times are minutes after local midnight in the schedule's zone — the
-- zone column, an IANA name, or NULL to mean the person's own profile
-- zone (users.timezone), falling back to UTC when that too is unknown.
-- Wall-clock minutes rather than instants: "09:00–17:00" must survive a
-- DST switch as 09:00–17:00, which a pair of UTC times cannot.
--
-- days is a bitmask, bit 0 = Monday … bit 6 = Sunday. Zero is a valid
-- schedule (someone who takes no meetings — every hour reads as outside
-- their hours), so only the range is constrained.
CREATE TABLE calendar_working_hours (
    tenant_id    TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    days         SMALLINT NOT NULL CHECK (days BETWEEN 0 AND 127),
    start_minute SMALLINT NOT NULL CHECK (start_minute BETWEEN 0 AND 1439),
    end_minute   SMALLINT NOT NULL CHECK (end_minute BETWEEN 1 AND 1440),
    zone         TEXT,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, user_id),
    CHECK (start_minute < end_minute)
);
