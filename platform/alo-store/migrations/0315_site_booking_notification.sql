-- The claim index of the booking-notification sweep (ADR 0036), the sibling of
-- `site_orders_pending_notification` in 0311.
--
-- The sweep asks the same question every thirty seconds — "which completed
-- appointments has nobody been told about?" — and the honest answer is almost
-- always none. Without this partial index that question is a scan of every
-- appointment a tenant has ever taken; with it, it is a read of the few rows
-- that are actually pending. `event_id IS NOT NULL` is part of the predicate
-- because it is part of the claim: a reservation whose Agenda event was never
-- written is one the visitor was never confirmed, and is not notified.

CREATE INDEX site_booking_appointments_pending_notification
    ON site_booking_appointments (created_at, id)
    WHERE notified_at IS NULL AND status = 'booked' AND event_id IS NOT NULL;
