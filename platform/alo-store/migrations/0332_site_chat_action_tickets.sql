-- The assistant's transcript learns the shop's offer (migration 0332,
-- ADR 0040/0041, item S3.04g): the conversation may now point a visitor at
-- one ticketed event's offer page, and the tenant's ledger records that act
-- as 'tickets_offered' — the fact being the event's name and day, both read
-- from the tenant's own price list and event row, never from the model.
--
-- Expand-only: the CHECK is widened in place; every stored word remains
-- valid and no row is touched.

ALTER TABLE site_chat_actions
    DROP CONSTRAINT site_chat_actions_kind_check;

ALTER TABLE site_chat_actions
    ADD CONSTRAINT site_chat_actions_kind_check CHECK (kind IN (
        'answered', 'refused', 'booking_offered', 'booked',
        'lead_offered', 'lead_saved', 'lead_known', 'tickets_offered'));
