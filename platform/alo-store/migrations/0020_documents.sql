-- Ficina Docs technical-authoring documents (ADR 0015). Tenant- and owner-scoped;
-- the block content is stored as JSONB. Every read/write is keyed on
-- (tenant_id, owner_id) so a document is reachable only by its owner.
CREATE TABLE documents (
    id         TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    tenant_id  TEXT        NOT NULL,
    owner_id   TEXT        NOT NULL,
    title      TEXT        NOT NULL,
    blocks     JSONB       NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX documents_owner_idx ON documents (tenant_id, owner_id, updated_at DESC);
