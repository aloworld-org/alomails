-- IMAP UID support (RFC 9051 §2.3.1.1). JMAP addresses messages by opaque
-- id; IMAP needs, per mailbox, a 32-bit strictly-ascending never-reused
-- UID and a stable UIDVALIDITY. These are the only IMAP-specific columns;
-- everything else the shim reads is existing account-scoped data.

-- UIDVALIDITY is drawn from one monotonic sequence, so every mailbox row
-- (including one created to replace a deleted same-named mailbox) gets a
-- distinct, never-reused validity value. Fits a 32-bit nz-number for the
-- deployment's life.
CREATE SEQUENCE mailbox_uidvalidity_seq AS BIGINT START 1;

ALTER TABLE mailboxes
    ADD COLUMN uid_validity BIGINT NOT NULL DEFAULT nextval('mailbox_uidvalidity_seq'),
    -- The next UID to hand out in this mailbox; monotone, only increments,
    -- never rewound (even by EXPUNGE) so UIDs are never reused.
    ADD COLUMN uid_next BIGINT NOT NULL DEFAULT 1;

-- Per-mailbox UID on each membership row (the same message in two
-- mailboxes has two independent UIDs).
ALTER TABLE mailbox_messages ADD COLUMN uid BIGINT;

-- Backfill existing memberships: assign UIDs in arrival order per mailbox.
WITH numbered AS (
    SELECT mailbox_id, message_id,
           row_number() OVER (PARTITION BY mailbox_id ORDER BY added_at, message_id) AS rn
    FROM mailbox_messages
)
UPDATE mailbox_messages mm
SET uid = numbered.rn
FROM numbered
WHERE mm.mailbox_id = numbered.mailbox_id
  AND mm.message_id = numbered.message_id;

-- Advance each mailbox's uid_next past its highest assigned UID so future
-- deliveries never collide with a backfilled UID.
UPDATE mailboxes mb
SET uid_next = COALESCE(
    (SELECT MAX(uid) + 1 FROM mailbox_messages mm WHERE mm.mailbox_id = mb.id),
    1
);

ALTER TABLE mailbox_messages ALTER COLUMN uid SET NOT NULL;
CREATE UNIQUE INDEX mailbox_messages_uid
    ON mailbox_messages(tenant_id, mailbox_id, uid);
