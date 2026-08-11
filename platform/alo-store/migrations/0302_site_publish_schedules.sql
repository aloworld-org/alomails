-- alo Sites S2.05a: publishing a website at a chosen moment instead of now.
-- A row is an *intention* to publish, not a publish: the immutable record of
-- what the internet served stays `site_publishes`, written by the ordinary
-- publish path when the worker claims a due row. Keeping the two apart is what
-- lets a schedule be cancelled, rescheduled, or fail without leaving a version
-- behind.
--
-- Lifecycle: scheduled → publishing → published | failed, plus cancelled from
-- `scheduled`. Terminal rows are retained so the tenant can see that a publish
-- happened (or why it did not) rather than watching a schedule vanish.

CREATE TABLE site_publish_schedules (
    tenant_id    TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id           TEXT NOT NULL,
    site_id      TEXT NOT NULL,
    publish_at   TIMESTAMPTZ NOT NULL,
    status       TEXT NOT NULL,
    -- The account door the publish will run through: a scheduled publish is
    -- made by somebody, and the resulting version records them as its author.
    requested_by TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    claimed_at   TIMESTAMPTZ,
    finished_at  TIMESTAMPTZ,
    attempts     INTEGER NOT NULL DEFAULT 0,
    -- The version this schedule produced, once it has produced one.
    publish_id   TEXT,
    last_error   TEXT,
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites (tenant_id, id) ON DELETE CASCADE,
    -- Composite, like every other pointer at a publish: a schedule can only
    -- ever name a version of its own tenant.
    FOREIGN KEY (tenant_id, publish_id)
        REFERENCES site_publishes (tenant_id, id),
    CONSTRAINT site_publish_schedules_status CHECK (
        status IN ('scheduled', 'publishing', 'published', 'cancelled', 'failed')),
    CONSTRAINT site_publish_schedules_attempts CHECK (attempts >= 0),
    CONSTRAINT site_publish_schedules_result CHECK (
        (status = 'published') = (publish_id IS NOT NULL))
);

-- At most one live intention per website. Rescheduling therefore updates the
-- pending row instead of racing a second one into existence, and two editors
-- clicking "schedule" at once cannot leave a site with two futures.
CREATE UNIQUE INDEX site_publish_schedules_pending_unique
    ON site_publish_schedules (tenant_id, site_id)
    WHERE status IN ('scheduled', 'publishing');

-- The sweeper's index: due rows first, across tenants.
CREATE INDEX site_publish_schedules_due
    ON site_publish_schedules (publish_at, id)
    WHERE status IN ('scheduled', 'publishing');

-- The tenant-facing history read.
CREATE INDEX site_publish_schedules_by_site
    ON site_publish_schedules (tenant_id, site_id, publish_at DESC, id);
