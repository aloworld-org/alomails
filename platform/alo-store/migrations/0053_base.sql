-- alo Base (ADR 0032): the relational data type — alo's native "sheet". A Base is
-- a drive_node (kind='base'); its data lives here, keyed back to that node. Access
-- is the Base node's Drive access (gated in the store), so a Base in a Space is
-- readable by members and writable by editors+ with no separate ACL. Everything
-- cascades with the tenant (Law 1).

-- A table within a Base.
CREATE TABLE base_tables (
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    id         TEXT NOT NULL,
    node_id    TEXT NOT NULL,          -- the Base's drive_node (kind='base')
    name       TEXT NOT NULL,
    position   DOUBLE PRECISION NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id)
);
CREATE INDEX base_tables_by_node ON base_tables (tenant_id, node_id);

-- A typed field (column) of a table.
--   type ∈ text | number | date | checkbox | select | multiselect | attachment
--         | person | link   (options JSONB carries select choices, link target, …)
CREATE TABLE base_fields (
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    id         TEXT NOT NULL,
    table_id   TEXT NOT NULL,
    name       TEXT NOT NULL,
    type       TEXT NOT NULL,
    options    JSONB NOT NULL DEFAULT '{}'::jsonb,
    position   DOUBLE PRECISION NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id)
);
CREATE INDEX base_fields_by_table ON base_fields (tenant_id, table_id);

-- A record (row). Cell values are JSONB keyed by field id (ADR 0032) — flexible
-- typing without a cell-per-row explosion.
CREATE TABLE base_records (
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    id         TEXT NOT NULL,
    table_id   TEXT NOT NULL,
    cells      JSONB NOT NULL DEFAULT '{}'::jsonb,
    position   DOUBLE PRECISION NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id)
);
CREATE INDEX base_records_by_table ON base_records (tenant_id, table_id);

-- A saved view over a table's records. Switching view never changes data.
--   kind ∈ grid | board | calendar | gallery   (config JSONB: fields, filters, …)
CREATE TABLE base_views (
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    id         TEXT NOT NULL,
    table_id   TEXT NOT NULL,
    kind       TEXT NOT NULL,
    name       TEXT NOT NULL,
    config     JSONB NOT NULL DEFAULT '{}'::jsonb,
    position   DOUBLE PRECISION NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id)
);
CREATE INDEX base_views_by_table ON base_views (tenant_id, table_id);
