-- The link in the mail that ends it (alo Campaigns, ADR 0044 §3; queue item
-- C2s.1).
--
-- ADR 0044 §3: "every campaign carries `List-Unsubscribe` with one-click
-- support (RFC 8058), and the link works without a login... a recipient who
-- cannot find the unsubscribe presses 'spam' instead, and that is the signal
-- that ends a sending reputation."
--
-- A link that works without a login is a link whose URL is the entire
-- credential, and that one sentence decides every column here.
--
-- THE TOKEN IS NEVER STORED. Only `sha256(token)` is, exactly as `file_shares`
-- (0026) and the meeting guest invitations (0210) hold theirs. A database dump,
-- a backup on somebody's laptop and a `SELECT *` over the shoulder are all read
-- access to this table, and none of them may hand over a working link to
-- somebody else's unsubscribe. The public route hashes what arrives and looks
-- the row up by the digest.
--
-- THE TOKEN IDENTIFIES; IT DOES NOT DESCRIBE. 256 random bits, minted per
-- recipient per send, encoding neither the address nor the send. The rejected
-- alternative is the one nearly every bulk sender ships — a signed or encoded
-- `?u=<customer id>&c=<campaign id>` — and it fails twice over: whoever holds
-- the mail can decode who else was sent it (an unsubscribe link is forwarded,
-- quoted in replies, and read by every scanner between us and the recipient),
-- and an id in a URL is an id somebody increments. Here there is nothing to
-- decode and nothing to increment: a wrong guess is a row that does not exist.
--
-- ONE ROW PER (SEND, RECIPIENT), AND OLD LINKS KEEP WORKING. There is
-- deliberately no unique constraint over (tenant_id, send_ref, address) and no
-- update path: minting again for the same person and the same send adds a
-- second row, and both links stay live. The token cannot be re-issued — we hold
-- only its digest — so the alternative is invalidating a link that is already
-- sitting in somebody's inbox, and a dead unsubscribe link is precisely the
-- thing that makes a person press the spam button instead. Rows accumulate;
-- one per recipient per send is the size of the send either way.
--
-- THERE IS NO EXPIRY, AND THAT IS THE DIFFERENCE FROM `file_shares`. A share
-- link is a file the sender chose to lend for a fortnight. This is a person's
-- ability to make us stop, and it must work when they find the mail two years
-- later in a search for something else. A column that would eventually turn an
-- unsubscribe into a 404 is a column that eventually earns a complaint.
--
-- WHAT IS DELIBERATELY MISSING: a foreign key on `send_ref`. It names the send
-- this link was minted for, and the per-recipient send record is queue item
-- C5m.1 — the same call `campaign_segments` (0502) made about
-- `received_campaign_id`. A reference to a table that does not exist is a
-- guess; the honest move is an opaque reference now and an additive FK when
-- there is something for it to point at. Nothing in this migration sends
-- anything, and nothing here can.
CREATE TABLE campaign_unsubscribe_tokens (
    -- `sha256(token)` as 64 lowercase hex characters, and the PRIMARY KEY —
    -- globally, not per tenant. The public route is reached with no account and
    -- no login, so the token is the only thing that names the tenant; a
    -- tenant-scoped key would need the caller to already know the answer.
    token_hash TEXT NOT NULL,
    tenant_id  TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    -- The record's handle, and the thing that gets written into
    -- `campaign_suppression.source_ref` when somebody uses the link. Never the
    -- token: an unsubscribe must be traceable to the send that caused it
    -- without the working credential being copied into a second table.
    id         TEXT NOT NULL,
    -- Which send. Opaque today (see above), and never shown to the recipient.
    send_ref   TEXT NOT NULL,
    -- The person, normalised exactly as every other campaign query normalises
    -- an address (`lower(btrim(...))`), so the suppression this link writes
    -- joins the audience rather than sitting beside it.
    address    TEXT NOT NULL,
    issued_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (token_hash),
    CONSTRAINT campaign_unsubscribe_tokens_id UNIQUE (tenant_id, id),
    -- The digest shape is held here rather than trusted to the caller: a row
    -- whose `token_hash` is a raw token would be a live link stored in
    -- plaintext, which is the one thing this table exists to prevent.
    CONSTRAINT campaign_unsubscribe_tokens_hash_shape CHECK (
        token_hash ~ '^[0-9a-f]{64}$'
    ),
    CONSTRAINT campaign_unsubscribe_tokens_send_ref CHECK (
        btrim(send_ref) <> '' AND char_length(send_ref) <= 200
    ),
    CONSTRAINT campaign_unsubscribe_tokens_address_normalised CHECK (
        address = lower(btrim(address)) AND address <> '' AND octet_length(address) <= 320
    )
);

-- Not a lookup path for the application — nothing above this file may find a
-- token from an address, because a query that can is an oracle for "is this
-- person on their list". It exists so dropping a tenant does not sequentially
-- scan every token ever minted, and so C5m.1 can count a send's links without
-- one.
CREATE INDEX campaign_unsubscribe_tokens_by_send
    ON campaign_unsubscribe_tokens (tenant_id, send_ref, address);
