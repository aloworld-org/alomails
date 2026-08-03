-- Store the Cc and Bcc recipients of a message (mail daily-driver). Cc is a
-- visible header present on every copy; Bcc is populated only on the sender's
-- own copy (the wire message has the Bcc header stripped at submission, so a
-- received message parses it as empty). Additive with empty defaults.
ALTER TABLE messages
    ADD COLUMN cc_addrs  TEXT NOT NULL DEFAULT '',
    ADD COLUMN bcc_addrs TEXT NOT NULL DEFAULT '';
