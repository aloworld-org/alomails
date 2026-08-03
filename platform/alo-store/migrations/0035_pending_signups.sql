-- Personal signup verification (ADR 0018, slice 3).
--
-- A pending self-service signup: an address a person has begun claiming but
-- not yet verified. Nothing here is a tenant/user yet — provisioning happens
-- only on successful verification, so an unverified attempt creates no
-- account. Keyed by the claimed address so a re-begin refreshes the same row,
-- and reaped on expiry.
CREATE TABLE pending_signups (
    -- The claimed address, lowercased (localpart@domain). One pending claim
    -- per address at a time.
    address        TEXT PRIMARY KEY,
    -- Where the verification code was sent (an existing external mailbox).
    recovery_email TEXT NOT NULL,
    -- SHA-256 at-rest hash of the (address-salted) verification code.
    code_hash      TEXT NOT NULL,
    -- Verify attempts, to cap online guessing of the short code.
    attempts       INTEGER NOT NULL DEFAULT 0,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at     TIMESTAMPTZ NOT NULL
);

-- Cheap expiry reaping.
CREATE INDEX pending_signups_expires ON pending_signups (expires_at);
