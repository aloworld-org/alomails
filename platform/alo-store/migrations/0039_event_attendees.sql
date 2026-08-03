-- Event guests (Agenda slice 4: invitations). The email addresses the owner
-- invited (iCalendar ATTENDEEs); on save the owner mails each an iMIP
-- invitation. Status tracking + RSVP replies are a later slice. Stored as a
-- JSONB array of addresses, defaulting to empty for existing events.
ALTER TABLE calendar_events ADD COLUMN attendees JSONB NOT NULL DEFAULT '[]'::jsonb;
