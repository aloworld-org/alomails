-- The campaign return path's intake log (mail M4.4, ADR 0044 §4): one row
-- per message that arrived at the configured bounce address
-- (ALO_SMTP_CAMPAIGN_RETURN_PATH). Host-level operational data like
-- dmarc_report_events — the address is a system mailbox shared by every
-- tenant's campaign mail, so the row has no tenant; the per-tenant
-- consequence of a hard bounce is written through campaign_suppression,
-- which is tenant-scoped, and this row is only the receipt.
--
-- The raw message is kept (bounded) because the one message that matters
-- most here is the one we could NOT parse: a provider's nonstandard bounce
-- format is diagnosed from the bytes, never from a verdict of 'none'.
CREATE TABLE campaign_bounces (
    id           TEXT PRIMARY KEY,
    -- hard: an RFC 3464 report of a settled permanent failure (Action:
    -- failed, Status 5.x.x) — the only verdict that suppresses.
    -- soft: a report of a transient condition (4.x.x or Action: delayed);
    -- the sender's own retry machinery is what acts on those.
    -- none: nothing to act on — not a delivery-status report at all, or
    -- one that reports only success.
    verdict      TEXT NOT NULL CHECK (verdict IN ('hard', 'soft', 'none')),
    -- The reported address the verdict is about (normalised), when the
    -- report named a usable one. NULL for verdict 'none'.
    recipient    TEXT CHECK (recipient IS NULL OR char_length(recipient) <= 320),
    -- The RFC 3463 enhanced status as reported (e.g. 5.1.1).
    status       TEXT CHECK (status IS NULL OR char_length(status) <= 32),
    -- How many tenant suppressions this message fired (0 for soft/none,
    -- and for a hard bounce of an address no tenant's campaign mailed).
    suppressed   INTEGER NOT NULL DEFAULT 0 CHECK (suppressed >= 0),
    -- The message as received, truncated to the store's cap; the true
    -- size on the wire is beside it so truncation is visible.
    message      BYTEA NOT NULL,
    message_size BIGINT NOT NULL CHECK (message_size >= 0),
    received_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- The operator's read is "what came back lately, and did it act".
CREATE INDEX campaign_bounces_window
    ON campaign_bounces (received_at DESC);
