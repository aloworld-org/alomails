-- Recurring events across DST: the IANA zone the series' wall-clock follows
-- (captured from a CalDAV DTSTART;TZID=… or the API's `timezone`), and the
-- extra occurrence instants an RDATE contributes. Both additive; existing
-- rows keep UTC-fixed expansion (tzid IS NULL) and no extra dates.
ALTER TABLE calendar_events
    ADD COLUMN IF NOT EXISTS tzid text,
    ADD COLUMN IF NOT EXISTS rdates jsonb NOT NULL DEFAULT '[]'::jsonb;
