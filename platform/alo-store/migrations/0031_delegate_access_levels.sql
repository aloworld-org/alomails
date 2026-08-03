-- Refine delegation grants (ADR 0017) to Outlook/Gmail parity: an access level
-- (read-only vs manage) and a send mode (none / send-as / send-on-behalf),
-- replacing the single can_send flag. `account_delegates` (migration 0030) is
-- new and empty, so this is a straight reshape.
ALTER TABLE account_delegates ADD COLUMN can_write BOOLEAN NOT NULL DEFAULT TRUE;
ALTER TABLE account_delegates ADD COLUMN send_mode TEXT NOT NULL DEFAULT 'none';

-- Carry any existing send grant to the new 'as' mode (no rows in practice).
UPDATE account_delegates SET send_mode = 'as' WHERE can_send;

ALTER TABLE account_delegates DROP COLUMN can_send;
ALTER TABLE account_delegates
    ADD CONSTRAINT account_delegates_send_mode CHECK (send_mode IN ('none', 'as', 'on_behalf'));
