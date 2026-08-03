-- Self-service password reset for personal accounts (ADR 0018 follow-up).
--
-- Two additive tables, no change to existing schema (expand):
--
-- 1. account_recovery — the recovery mailbox captured at signup, kept so a
--    forgotten password can be reset by mailing a code to it. One row per
--    account, keyed by its address (= credential username). Written at
--    provisioning; a personal account has exactly one.
--
-- 2. pending_resets — the short-lived reset-in-progress state: a code mailed to
--    the recovery address, awaiting verification. Mirrors pending_signups.
--    Keyed by the account address so a re-request refreshes the same row, and
--    reaped on expiry. Holds no password — the new password arrives only on the
--    verify call and is hashed straight into `credentials`.
CREATE TABLE account_recovery (
    -- The account's address, lowercased (localpart@domain) = credential username.
    address        TEXT PRIMARY KEY,
    tenant_id      TEXT NOT NULL,
    user_id        TEXT NOT NULL,
    -- The external mailbox to send a reset code to.
    recovery_email TEXT NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE pending_resets (
    -- The account address a reset is in progress for. One at a time.
    address        TEXT PRIMARY KEY,
    -- Where the reset code was sent (copied from account_recovery at request).
    recovery_email TEXT NOT NULL,
    -- SHA-256 at-rest hash of the (address-salted) reset code.
    code_hash      TEXT NOT NULL,
    -- Verify attempts, to cap online guessing of the short code.
    attempts       INTEGER NOT NULL DEFAULT 0,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at     TIMESTAMPTZ NOT NULL
);

-- Cheap expiry reaping.
CREATE INDEX pending_resets_expires ON pending_resets (expires_at);
