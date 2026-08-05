-- Drive: the file tree (ADR 0027). Every node — folder, file, or document —
-- lives in exactly one location: a user's personal "My Files"
-- (location_kind='personal', location_id=user id) or a Space
-- (location_kind='space', location_id=space id). Access follows location; there
-- is no per-node permission. Bytes live in the blob store (Garage); a node just
-- references a blob. Everything cascades with the tenant (Law 1).

CREATE TABLE drive_nodes (
    tenant_id     TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    id            TEXT NOT NULL,
    location_kind TEXT NOT NULL,          -- 'personal' | 'space'
    location_id   TEXT NOT NULL,          -- user id or space id
    parent_id     TEXT,                   -- NULL = a location root
    kind          TEXT NOT NULL,          -- 'folder' | 'file' | 'doc' | 'sheet' | 'slides'
    name          TEXT NOT NULL,
    blob_id       TEXT,                   -- NULL for folders
    size          BIGINT NOT NULL DEFAULT 0,
    content_type  TEXT,
    trashed       BOOLEAN NOT NULL DEFAULT false,
    -- Optional jump-back to the email/task/event a file came from (ADR 0029).
    source_kind   TEXT,
    source_id     TEXT,
    created_by    TEXT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id)
);
-- List a folder within a location.
CREATE INDEX drive_nodes_by_parent
    ON drive_nodes (tenant_id, location_kind, location_id, parent_id);
-- The trash view + whole-location scans.
CREATE INDEX drive_nodes_by_location_trashed
    ON drive_nodes (tenant_id, location_kind, location_id, trashed);

-- Version history (ADR 0027): every upload/save appends a version; the node's
-- current blob_id is the latest. Restore appends a NEW version pointing at an
-- old blob, so history is never rewritten.
CREATE TABLE drive_node_versions (
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    node_id    TEXT NOT NULL,
    version_no INTEGER NOT NULL,
    blob_id    TEXT NOT NULL,
    size       BIGINT NOT NULL,
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, node_id, version_no)
);
