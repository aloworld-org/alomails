-- Mail filters (server-side rules): the user's structured filter rules, stored
-- as an opaque JSON array. ficina-jmap owns the rule model and compiles the
-- rules (together with any out-of-office vacation) into a single managed Sieve
-- script that the existing delivery-time evaluator runs. Storing the structured
-- form lets the settings UI round-trip the rules for editing.
ALTER TABLE user_settings
    ADD COLUMN filters TEXT NOT NULL DEFAULT '[]';
