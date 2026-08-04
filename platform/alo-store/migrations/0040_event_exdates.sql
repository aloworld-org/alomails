-- Per-occurrence exceptions (Agenda: recurring exceptions). The excluded
-- occurrence start-times of a recurring event (iCalendar EXDATE) — the dates
-- that were individually skipped/cancelled while the rest of the series stays.
-- Stored as a JSONB array of RFC 3339 UTC timestamps, defaulting to empty for
-- existing events. Editing a single occurrence in place (RECURRENCE-ID
-- overrides) is a later slice that builds on this.
ALTER TABLE calendar_events ADD COLUMN exdates JSONB NOT NULL DEFAULT '[]'::jsonb;
