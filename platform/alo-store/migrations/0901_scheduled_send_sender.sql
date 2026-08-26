-- Scheduled sends from a shared mailbox (ADR 0017): when a delegate schedules
-- an on-behalf send, the acting delegate's address must survive until the
-- sweeper puts the message on the wire — the Sender: header is the disclosure
-- on-behalf sending exists for, and the draft's stored bytes do not carry it.
-- Nullable: an ordinary send (or send-as) has no acting sender to disclose.
ALTER TABLE scheduled_sends
    ADD COLUMN IF NOT EXISTS on_behalf_sender TEXT;
