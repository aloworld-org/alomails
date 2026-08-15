-- The visitor assistant's appearance and voice (migration 0325, ADR 0040 §5,
-- item S3.02f). Everything a tenant may change about the widget is content or
-- a bounded choice — welcome message, bot name and avatar, suggested opening
-- questions, a tone note, launcher corner and icon, the offline message, and
-- an accent chosen among the site's own palette roles — never free-form CSS,
-- colours, or fonts.
--
-- Stored as one versioned JSON envelope beside the assistant's switch and
-- ceiling: the settings row is already "the tenant's choices about the
-- assistant", and a second table would be a second thing to keep consistent.
-- A pristine '{}' reads as the defaults (the widget wears the site's theme
-- and speaks our localized copy), exactly like a site's theme column.

ALTER TABLE site_chat_settings
    ADD COLUMN appearance JSONB NOT NULL DEFAULT '{}'::jsonb;
