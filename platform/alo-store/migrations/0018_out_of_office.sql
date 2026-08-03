-- Out-of-office (vacation) settings, stored alongside the user's other mail
-- settings. The auto-reply itself is delivered by the existing Sieve vacation
-- machinery: toggling this on installs and activates a managed `out-of-office`
-- Sieve script; toggling off removes it. Additive with safe defaults.
ALTER TABLE user_settings
    ADD COLUMN ooo_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN ooo_subject TEXT NOT NULL DEFAULT '',
    ADD COLUMN ooo_message TEXT NOT NULL DEFAULT '';
