-- Recurring events (Agenda slice 3): an optional iCalendar RRULE on an event.
-- `starts_at`/`ends_at` describe the first occurrence; the store expands the
-- rest within a queried range, and CalDAV round-trips the raw RRULE so native
-- clients expand it themselves. Additive column, no backfill (existing events
-- are one-offs).
ALTER TABLE calendar_events ADD COLUMN rrule TEXT;
