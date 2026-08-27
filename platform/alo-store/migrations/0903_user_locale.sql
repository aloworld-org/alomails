-- Server-synced interface language (mail M4.2): the language the user
-- last chose in the switcher, kept on their settings row so every device
-- they sign in on speaks it. NULL means they have never chosen — the
-- client then falls back to browser detection, exactly as before this
-- column existed.
ALTER TABLE user_settings ADD COLUMN locale TEXT;
