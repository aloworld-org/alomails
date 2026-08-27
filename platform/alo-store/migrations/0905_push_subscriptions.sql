-- Web Push subscriptions (mail M5.3): one row per user+device, the handle a
-- browser's push service gave us for that installation. The endpoint URL and
-- the client's ECDH public key + auth secret (RFC 8291) are stored verbatim —
-- they encrypt TOWARD the browser and unlock nothing of ours — and deleting
-- the row is the whole of unsubscribing on our side. Payloads built from
-- these rows carry counts and ids only, never message content, so the row
-- grants the push service no reach into mail data either way.
CREATE TABLE push_subscriptions (
    id         TEXT PRIMARY KEY,
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    endpoint   TEXT NOT NULL,
    p256dh     TEXT NOT NULL,
    auth       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- One row per device: re-subscribing the same browser replaces the keys
    -- rather than piling up dead rows for one machine.
    UNIQUE (tenant_id, user_id, endpoint)
);
CREATE INDEX push_subscriptions_user ON push_subscriptions(tenant_id, user_id);
