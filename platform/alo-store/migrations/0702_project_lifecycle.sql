-- A project is more than the container its tasks happen to live in. Keep the
-- small set of lifecycle facts every project view needs on the project itself,
-- so portfolio, overview and timeline never derive competing answers.
ALTER TABLE task_projects
    ADD COLUMN description TEXT,
    ADD COLUMN status TEXT NOT NULL DEFAULT 'active',
    ADD COLUMN starts_on DATE,
    ADD COLUMN target_on DATE,
    ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ADD CONSTRAINT task_projects_status_check
        CHECK (status IN ('planned', 'active', 'on_hold', 'completed', 'cancelled')),
    ADD CONSTRAINT task_projects_dates_check
        CHECK (target_on IS NULL OR starts_on IS NULL OR target_on >= starts_on);
