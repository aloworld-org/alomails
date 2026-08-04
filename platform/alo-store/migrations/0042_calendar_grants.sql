-- Calendar sharing (Agenda: team/shared calendars, slice 2). A grant gives a
-- subject — an individual user OR a whole group — view or edit access to a
-- calendar it does not own. One table handles both sharing models: a grant to a
-- `user` is share-with-a-person; a grant to a `group` is team/group access
-- (every member of the group inherits it).
--
-- A user's visible calendars = ones they own + ones granted to them directly +
-- ones granted to any group they belong to (group_members, 0008). Edit access =
-- owner, or a grant with role 'editor' (direct or via a group).

CREATE TABLE calendar_grants (
    tenant_id    TEXT NOT NULL,
    calendar_id  TEXT NOT NULL,
    -- 'user' or 'group'.
    subject_kind TEXT NOT NULL,
    -- The granted user id or group id.
    subject_id   TEXT NOT NULL,
    -- 'viewer' (read) or 'editor' (read + write).
    role         TEXT NOT NULL DEFAULT 'viewer',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, calendar_id, subject_kind, subject_id)
);

-- "Which calendars can this subject see?" — the hot path for building a user's
-- calendar list.
CREATE INDEX calendar_grants_by_subject
    ON calendar_grants (tenant_id, subject_kind, subject_id);
