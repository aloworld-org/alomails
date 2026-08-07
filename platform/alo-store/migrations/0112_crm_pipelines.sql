-- alo CRM (ADR 0035, wave B2): the boards a tenant's deals move across.
--
-- A pipeline is TENANT-WIDE and a tenant may have several — New Business,
-- Renewals, one per sales team (docs/design/crm.md, "Pipelines and stages").
-- There is deliberately no per-pipeline access boundary in B2: the
-- cross-cutting roles feature attaches to this table additively, with its own
-- migration, rather than being half-settled here by a nullable column.
--
-- Pipelines and stages are ARCHIVED, never deleted (the one exception is a
-- stage created by mistake, which no deal and no history row has ever named —
-- enforced in the store, alo_store::crm_stages). A deal closed last year must
-- always be able to name the board and the column it closed in.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE crm_pipelines (
    tenant_id   TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id          TEXT NOT NULL,
    -- The board's label. Unique per tenant among the active pipelines (see
    -- the index below) — two boards called "Sales" have no meaning to the
    -- person reading the tabs, and the uniqueness is also what makes the
    -- first-use seed race-free.
    name        TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    -- NULL = active. Archiving hides the board from the pickers without
    -- rewriting the deals that were won on it.
    archived_at TIMESTAMPTZ,
    created_by  TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- Defence in depth: the store validates before writing, so a violation
    -- here means a bug, not user input.
    CONSTRAINT crm_pipelines_name_shape CHECK (length(btrim(name)) > 0)
);

-- The list surface, and the rule that one active board owns one name. The
-- predicate lets an archived "Sales" coexist with a new one, which is what
-- archiving is for.
CREATE UNIQUE INDEX crm_pipelines_active_name
    ON crm_pipelines (tenant_id, lower(name))
    WHERE archived_at IS NULL;

CREATE TABLE crm_stages (
    tenant_id   TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id          TEXT NOT NULL,
    pipeline_id TEXT NOT NULL,
    -- The column header. User data from the moment it is seeded: renaming
    -- "Qualified" is a rename, not a schema change, which is exactly why the
    -- board's MEANING lives in the two flags below and not in the names.
    name        TEXT NOT NULL,
    -- Fractional ordering, the same shape a task carries on a board
    -- (ADR 0022). An ordering, not a quantity — the only non-integer column
    -- alo CRM has, and it never touches money.
    position    DOUBLE PRECISION NOT NULL DEFAULT 0,
    -- What makes a column mean "closed". A stage may set at most one.
    is_won      BOOLEAN NOT NULL DEFAULT FALSE,
    is_lost     BOOLEAN NOT NULL DEFAULT FALSE,
    archived_at TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- Within the tenant, always: a stage can only ever belong to a pipeline
    -- of its own tenant, and it goes when the board does.
    FOREIGN KEY (tenant_id, pipeline_id)
        REFERENCES crm_pipelines (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT crm_stages_name_shape CHECK (length(btrim(name)) > 0),
    CONSTRAINT crm_stages_one_outcome CHECK (NOT (is_won AND is_lost))
);

-- The board surface: one pipeline's columns, left to right.
CREATE INDEX crm_stages_by_pipeline ON crm_stages (tenant_id, pipeline_id, position);

-- A board with two "Won" columns has no win rate. Archived stages are
-- included on purpose: a closed deal still points at the column it closed in,
-- so the flag stays spoken for until somebody clears it.
CREATE UNIQUE INDEX crm_stages_one_won
    ON crm_stages (tenant_id, pipeline_id) WHERE is_won;
CREATE UNIQUE INDEX crm_stages_one_lost
    ON crm_stages (tenant_id, pipeline_id) WHERE is_lost;
