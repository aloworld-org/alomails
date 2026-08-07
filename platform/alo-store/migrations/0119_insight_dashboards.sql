-- alo Insights (ADR 0037, wave BI-1): the boards a tenant reads its numbers
-- from, and the tiles pinned to them.
--
-- A dashboard is TENANT-WIDE in BI-1 — every member of a tenant sees every
-- board. Spaces-scoped sharing is real and wanted (ADR 0037), but it is the
-- same cross-cutting role question B4.12 owns, where the accountant is the
-- first scoped role; deciding it from its narrowest caller would settle that
-- design by accident (docs/design/insights.md, "Tenancy").
--
-- NOTHING COMPUTED IS STORED. There is no results table, no snapshot and no
-- cache here on purpose: a stored subtotal outlives the rows that justify it,
-- and a fast number that disagrees with the invoice underneath it is worse
-- than a slow one. A tile holds the QUESTION (its ChartSpec); the answer is
-- evaluated from the documents every time.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE insight_dashboards (
    tenant_id   TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id          TEXT NOT NULL,
    -- The board's label, in the tenant's own words.
    name        TEXT NOT NULL,
    -- NULL for a board a user made. A non-NULL key names a board WE seeded —
    -- 'business_overview' is the only one BI-1 mints (BI1.06). It exists
    -- solely so the seed runs once per tenant: from the moment it exists a
    -- seeded board is an ORDINARY board, renamable, with tiles addable and
    -- removable. The partial unique index below is what makes the seed
    -- idempotent and race-free without a lock.
    system_key  TEXT,
    created_by  TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- Defence in depth: the store validates before writing, so a violation
    -- here means a bug, not user input.
    CONSTRAINT insight_dashboards_name_shape CHECK (length(btrim(name)) > 0)
);

-- One seeded board of each kind per tenant, ever. Partial, because user-made
-- boards carry no key and must not compete for the same slot.
CREATE UNIQUE INDEX insight_dashboards_system_key
    ON insight_dashboards (tenant_id, system_key)
    WHERE system_key IS NOT NULL;

-- The list surface: a tenant's boards, oldest first (the seeded overview is
-- the first board a tenant ever has, so it stays the first tab).
CREATE INDEX insight_dashboards_by_tenant
    ON insight_dashboards (tenant_id, created_at, id);

CREATE TABLE insight_tiles (
    tenant_id    TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id           TEXT NOT NULL,
    dashboard_id TEXT NOT NULL,
    -- What the tile is called on the board. User data.
    title        TEXT NOT NULL,
    -- The ChartSpec envelope: a typed question over the closed catalog, never
    -- a query and never SQL (alo_store::insight_spec). Validated against the
    -- typed model on every write, exactly the way a site page's sections are —
    -- what lands here always round-trips through the Rust types.
    spec         JSONB NOT NULL,
    -- The chart form, DERIVED from the spec on write and never taken from the
    -- caller separately, so the column cannot drift from the envelope it
    -- summarises. It is here so a board can be laid out — and a tile whose
    -- spec a future version wrote can still be drawn as a placeholder of the
    -- right shape — without parsing every spec first.
    viz          TEXT NOT NULL,
    -- Fractional ordering, the same shape a task board carries (ADR 0022): an
    -- ordering, never a quantity. Insights holds no money in this table at
    -- all, so this is the only non-integer column it has.
    position     DOUBLE PRECISION NOT NULL DEFAULT 0,
    -- How many of the four grid columns the tile occupies. A typed layout is
    -- one an AI can also write, which is why BI-1 has a span and not a canvas.
    span         SMALLINT NOT NULL DEFAULT 1,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- Within the tenant, always: a tile can only ever belong to a board of its
    -- own tenant, and it goes when the board does.
    FOREIGN KEY (tenant_id, dashboard_id)
        REFERENCES insight_dashboards (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT insight_tiles_title_shape CHECK (length(btrim(title)) > 0),
    CONSTRAINT insight_tiles_span_range CHECK (span BETWEEN 1 AND 4)
);

-- The board surface: one dashboard's tiles, in layout order.
CREATE INDEX insight_tiles_by_dashboard
    ON insight_tiles (tenant_id, dashboard_id, position);
