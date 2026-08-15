-- The visitor's half of a reservation: a capability token that lets whoever
-- holds it see, calendar-import, and cancel exactly one appointment — and
-- nothing else. It is minted at reservation time and travels only in the
-- confirmation the visitor is shown (the conversation card, the confirmation
-- page, the .ics description), never in any listing.
--
-- Cancellation is what makes the bot's booking a *reversible* act in the
-- ADR 0040 §2 sense: a meeting the assistant booked wrongly is one the
-- visitor can undo themselves, without an account and without calling anyone.
--
-- Expand-only: existing appointments keep NULL and are simply not manageable
-- this way; every new reservation carries a token.

ALTER TABLE site_booking_appointments
    ADD COLUMN manage_token TEXT;

-- The lookup is by bare token (that is the whole point of a capability), so
-- it must be unique across all tenants; the serving site still scopes the
-- read, so a token can only ever be used on the host it was minted for.
CREATE UNIQUE INDEX site_booking_appointments_by_manage_token
    ON site_booking_appointments (manage_token)
    WHERE manage_token IS NOT NULL;
