-- Out-of-office gets a date window, so it can be set before leaving.
--
-- WHY. Today the auto-reply is on or off, and only a person can move it
-- between the two. That is not how anybody uses it: you set it the evening
-- before you leave and you want it to stop by itself when you are back. The
-- roadmap item has always read "out-of-office **with scheduling**"; this is the
-- scheduling.
--
-- It also closes a quieter defect. alo advertises the JMAP
-- `urn:ietf:params:jmap:vacationresponse` capability, whose VacationResponse
-- object (RFC 8621 section 8) carries `fromDate` and `toDate` as UTCDate|null.
-- We accepted both and reported both as null, always: a standards-following
-- client that scheduled a holiday had its dates silently dropped, and either
-- got no reply at all or one that never stopped. Advertising a capability and
-- discarding half of it is worse than not advertising it.
--
-- WHY NULL MEANS UNBOUNDED, on each side independently. RFC 8621 makes both
-- properties nullable, and the two nulls mean different useful things: no
-- `fromDate` is "starting now", no `toDate` is "until I say otherwise" — which
-- is exactly today's behaviour, so every existing row keeps working unchanged
-- without a value being invented for it.
--
-- WHY NOTHING IS SCHEDULED TO RUN. There is no timer that switches the reply on
-- and off. The window is read when a message arrives and the reply is decided,
-- which is the only moment it matters. A scheduler would be a second thing that
-- can be down, and being down would mean either replying to somebody who should
-- have had a person, or going silent while somebody is away.
--
-- Expand-only: two nullable columns, no default, no existing row touched.
ALTER TABLE user_settings
    ADD COLUMN ooo_from TIMESTAMPTZ,
    ADD COLUMN ooo_to   TIMESTAMPTZ;

COMMENT ON COLUMN user_settings.ooo_from IS
    'When the out-of-office reply starts; NULL means it is already in effect.';
COMMENT ON COLUMN user_settings.ooo_to IS
    'When it stops; NULL means it runs until switched off by hand.';

-- A window that ends before it starts would silently never fire, which reads
-- to the person who set it exactly like the feature being broken.
ALTER TABLE user_settings
    ADD CONSTRAINT user_settings_ooo_window_ordered
    CHECK (ooo_from IS NULL OR ooo_to IS NULL OR ooo_from < ooo_to);
