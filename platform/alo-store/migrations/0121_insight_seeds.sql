-- alo Insights (ADR 0037, wave BI1.06): the ledger of which prebuilt boards a
-- tenant has already been given.
--
-- The Business overview is written once per tenant, on the first read of
-- /insights/dashboards. "Once" has to survive the board itself: a tenant that
-- throws the overview away must not be handed a new one the next morning, and
-- the dashboard row cannot answer that question after it has been deleted.
-- So the fact that the seed RAN is recorded here, separately from what it
-- wrote, and it is never removed while the tenant exists.
--
-- The primary key is what makes the seed race-free without a lock: two first
-- visits at the same instant both try to insert this row, exactly one wins
-- (ON CONFLICT DO NOTHING), and the winner is the transaction that writes the
-- board and its tiles.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE insight_seeds (
    tenant_id   TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    -- Which prebuilt board: 'business_overview' is the only key BI-1 mints.
    -- Ours, never a caller's (alo_store::insight_dashboards::normalize_key).
    system_key  TEXT NOT NULL,
    -- Whoever opened Insights first. The seed's board carries the same name in
    -- its created_by, but this row outlives that board.
    seeded_by   TEXT NOT NULL,
    seeded_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, system_key)
);
