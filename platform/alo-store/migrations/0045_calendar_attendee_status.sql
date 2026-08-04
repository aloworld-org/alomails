-- Organizer-side RSVP tracking (Agenda). When a guest replies to an invitation
-- (an iMIP REPLY), the organizer's copy of the event records that guest's
-- participation status, so the event shows who accepted / declined / is tentative
-- rather than the reply merely landing as an email. A JSONB map of
-- attendee-email -> PARTSTAT (ACCEPTED | DECLINED | TENTATIVE | NEEDS-ACTION);
-- written only by applying a reply, and preserved across ordinary event edits.
ALTER TABLE calendar_events
    ADD COLUMN attendee_status JSONB NOT NULL DEFAULT '{}'::jsonb;
