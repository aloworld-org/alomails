-- A follow-up due-date on a flagged message (Outlook-style "flag with
-- reminder"): when set, the flag carries a date the user means to act by. It is
-- a plain nullable timestamp on the message — no reminder/sweeper is implied;
-- the UI shows the date and marks it overdue. Mirrors 0021's snooze column.
ALTER TABLE messages ADD COLUMN flag_due TIMESTAMPTZ;

-- Partial index: only the handful of messages that actually carry a due-date,
-- for the flagged view's "soonest first / overdue" ordering.
CREATE INDEX messages_flag_due_idx ON messages (flag_due) WHERE flag_due IS NOT NULL;
