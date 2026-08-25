-- A signature per send identity, not one per account.
--
-- WHY. A user who sends as support@ and as their own name signs those two
-- differently — "The support team" under one, their own name under the other.
-- Until now every identity shared the single account signature, and RFC 8621
-- §6 (Identity: textSignature/htmlSignature) advertised a per-identity fact we
-- could not store: a standard client that set a signature on one identity had
-- it silently applied to none.
--
-- One row per (user, address). No row means "no per-identity signature", which
-- falls back to the account-level signature in user_settings — so every
-- existing account behaves exactly as before without a value being invented.
-- Both spellings are stored because RFC 8621 carries both and a client that
-- round-trips textSignature must get its own text back, not a conversion.
--
-- Expand-only: a new table, no existing row touched.
CREATE TABLE identity_signatures (
    tenant_id      TEXT        NOT NULL,
    user_id        TEXT        NOT NULL,
    address        TEXT        NOT NULL,
    text_signature TEXT        NOT NULL DEFAULT '',
    html_signature TEXT        NOT NULL DEFAULT '',
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, user_id, address)
);

COMMENT ON TABLE identity_signatures IS
    'Per-send-identity mail signatures (RFC 8621 Identity); absence falls back to user_settings.signature.';
