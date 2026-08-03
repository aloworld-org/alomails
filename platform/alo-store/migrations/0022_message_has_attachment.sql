-- Whether a message carries an attachment, so the list can show a paperclip
-- without loading and MIME-parsing the body. Nullable: NULL means "not yet
-- computed" (existing rows), which a one-time backfill fills in; ingest sets it
-- for new mail.
ALTER TABLE messages ADD COLUMN has_attachment BOOLEAN;

-- The backfill scans rows still to be computed.
CREATE INDEX messages_has_attachment_null_idx ON messages (id) WHERE has_attachment IS NULL;
