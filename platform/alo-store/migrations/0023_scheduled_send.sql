-- Send later (Gmail-style scheduled send): a composed draft is held until a
-- chosen time instead of going out immediately. The draft is moved to the
-- account's Scheduled mailbox and the validated envelope + wake time are
-- recorded here; a background sweeper submits it through the normal outbound
-- path when due, then files it to Sent. Cancelling deletes the row and returns
-- the draft to Drafts. The envelope is stored validated at schedule time so the
-- sweeper never has to re-derive send-from rights.
CREATE TABLE scheduled_sends (
    tenant_id  TEXT        NOT NULL,
    user_id    TEXT        NOT NULL,
    message_id TEXT        NOT NULL,
    send_at    TIMESTAMPTZ NOT NULL,
    mail_from  TEXT        NOT NULL,
    rcpts      TEXT[]      NOT NULL,
    PRIMARY KEY (tenant_id, user_id, message_id)
);

-- The sweeper scans by due time across tenants; index it.
CREATE INDEX scheduled_sends_due_idx ON scheduled_sends (send_at);
