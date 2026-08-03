-- The address book: per-account contacts (JMAP Contacts / CardDAV source
-- of truth). One row per contact; multi-valued emails and phones are JSON
-- arrays of {kind, value} so a contact round-trips to a vCard without a
-- child table on the read path. Tenant + user scoped like every other
-- account-owned object.
CREATE TABLE contacts (
    tenant_id    TEXT        NOT NULL,
    user_id      TEXT        NOT NULL,
    id           TEXT        NOT NULL PRIMARY KEY,
    -- The formatted name shown everywhere (vCard FN — required, never empty).
    display_name TEXT        NOT NULL,
    first_name   TEXT,
    last_name    TEXT,
    -- [{"kind": "work"|"home"|null, "value": "a@b"}]; validated in Rust.
    emails       JSONB       NOT NULL DEFAULT '[]',
    phones       JSONB       NOT NULL DEFAULT '[]',
    organization TEXT,
    job_title    TEXT,
    notes        TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Address-book listings and the autocomplete merge scan by owner, name-ordered.
CREATE INDEX contacts_owner ON contacts (tenant_id, user_id, display_name);
