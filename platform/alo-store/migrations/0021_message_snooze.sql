-- Snooze (Gmail-style): a message can be hidden until a chosen time. The
-- snoozed message is moved to the account's Snoozed mailbox and carries the
-- wake time here; a background sweeper returns it to the Inbox when due.
ALTER TABLE messages ADD COLUMN snooze_until TIMESTAMPTZ;

-- Partial index: the sweeper only ever scans currently-snoozed rows.
CREATE INDEX messages_snooze_idx ON messages (snooze_until) WHERE snooze_until IS NOT NULL;
