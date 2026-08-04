-- Event reminders (Agenda: "notify me 10 minutes before"). A single lead-time
-- per event, in minutes before the start; NULL means no reminder. Stored on the
-- event and serialised to a VALARM over CalDAV, so the alert fires natively on
-- the user's phone/Apple Calendar even when the web app is closed. A recurring
-- series shares one reminder across its occurrences.
ALTER TABLE calendar_events ADD COLUMN reminder_minutes INTEGER;
