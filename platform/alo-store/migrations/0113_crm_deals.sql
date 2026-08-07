-- alo CRM (ADR 0035, wave B2): the deals that move across the boards, and the
-- append-only history of every move they ever made.
--
-- A deal names a `billing_customers` row when the company is already one the
-- tenant invoices, and carries its own company/contact columns while it is
-- still a lead — shaped like the customer's, because winning such a deal
-- creates one from exactly those fields (docs/design/crm.md, "The customer,
-- the lead, and the contact"). CRM deliberately does NOT grow a second
-- organisation table beside the customer.
--
-- The closing snapshot (`outcome`, `lost_reason`, `closed_at`) is written onto
-- the deal at the moment it moves into a flagged column, so re-flagging a
-- stage next year never rewrites last year's win rate — the same reason a
-- billing line snapshots its price instead of joining to the price list.
--
-- Money is integer cents, always (Law: no float touches money). The only
-- DOUBLE PRECISION here is `position`, which is an ordering, not a quantity.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE crm_deals (
    tenant_id      TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id             TEXT NOT NULL,
    -- The board and the column the deal is on right now. The column is
    -- re-checked against the board on every move: a deal may not be lost into
    -- another team's funnel.
    pipeline_id    TEXT NOT NULL,
    stage_id       TEXT NOT NULL,
    -- What the opportunity is. Required — a deal without one is a row nobody
    -- can read on a board.
    title          TEXT NOT NULL,
    -- The company the tenant already invoices, or NULL while this is a lead.
    customer_id    TEXT,
    -- A convenience pointer into the linking user's own address book.
    -- Contacts are PER USER (contacts.user_id), while a deal is tenant-wide,
    -- so this may simply not resolve for a colleague reading the deal — which
    -- is exactly why the name and email the whole team must see are columns
    -- here rather than a join.
    contact_id     TEXT,
    company_name   TEXT NOT NULL DEFAULT '',
    contact_name   TEXT NOT NULL DEFAULT '',
    contact_email  TEXT NOT NULL DEFAULT '',
    -- What the deal is worth, in integer cents of `currency`. A negative deal
    -- value is not a discount, it is a typo (CHECK below).
    value_cents    BIGINT NOT NULL DEFAULT 0,
    -- ISO 4217, uppercased in the store. The pipeline report groups BY this
    -- rather than converting: a forecast has no issue date, so there is no
    -- honest rate to convert it at (docs/design/crm.md).
    currency       TEXT NOT NULL DEFAULT 'EUR',
    -- The day the deal is expected to close, or NULL when nobody has said.
    expected_close DATE,
    -- Whose deal it is. A user of this tenant, checked in the store.
    owner_user_id  TEXT NOT NULL,
    -- Where the opportunity came from ('referral', 'website', …). Free text:
    -- a tenant's own vocabulary, not ours.
    source         TEXT NOT NULL DEFAULT '',
    -- Fractional order WITHIN the stage column, the same shape a task card
    -- carries on a board (ADR 0022). An ordering, never a quantity.
    position       DOUBLE PRECISION NOT NULL DEFAULT 0,
    -- The closing snapshot. NULL outcome = the deal is open; a reopened deal
    -- clears all three and keeps both moves in its history.
    outcome        TEXT,
    lost_reason    TEXT,
    closed_at      TIMESTAMPTZ,
    created_by     TEXT NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- Within the tenant, always: a deal's board and column are its own
    -- tenant's, so a guessed id from another tenant cannot be linked.
    FOREIGN KEY (tenant_id, pipeline_id)
        REFERENCES crm_pipelines (tenant_id, id) ON DELETE CASCADE,
    -- RESTRICT, not CASCADE: a column a deal stands in may not be deleted out
    -- from under it. The store answers that attempt with a clean Conflict
    -- (alo_store::crm_stages::delete_crm_stage); this is the backstop.
    CONSTRAINT crm_deals_stage_fk FOREIGN KEY (tenant_id, stage_id)
        REFERENCES crm_stages (tenant_id, id) ON DELETE RESTRICT,
    -- The same shape billing's own documents use for the customer link
    -- (0102_billing_invoices.sql). Customers are archived, never deleted, so
    -- in practice this cascades only with the tenant.
    CONSTRAINT crm_deals_customer_fk FOREIGN KEY (tenant_id, customer_id)
        REFERENCES billing_customers (tenant_id, id) ON DELETE CASCADE,
    -- Deleting an address-book entry unlinks it; it never destroys the deal.
    CONSTRAINT crm_deals_contact_fk
        FOREIGN KEY (contact_id) REFERENCES contacts (id) ON DELETE SET NULL,
    -- Defence in depth: the store validates every one of these before writing,
    -- so a violation here means a bug in our code, not bad user input.
    CONSTRAINT crm_deals_title_shape CHECK (length(btrim(title)) > 0),
    CONSTRAINT crm_deals_value_range
        CHECK (value_cents >= 0 AND value_cents <= 100000000000),
    CONSTRAINT crm_deals_currency_shape CHECK (currency ~ '^[A-Z]{3}$'),
    CONSTRAINT crm_deals_outcome_known
        CHECK (outcome IS NULL OR outcome IN ('won', 'lost')),
    -- A closing snapshot is whole or absent: a deal is never closed without a
    -- time, and never timed without an outcome.
    CONSTRAINT crm_deals_closed_together
        CHECK ((outcome IS NULL) = (closed_at IS NULL)),
    -- A lost reason belongs to a lost deal, and a lost deal always has one:
    -- "lost reasons + win/loss reporting" is the feature, and a reason that is
    -- optional is a reason nobody enters. `IS NOT DISTINCT FROM` keeps the
    -- comparison boolean when `outcome` is NULL, where plain `=` would be
    -- unknown and let the row through.
    CONSTRAINT crm_deals_lost_reason_together
        CHECK ((outcome IS NOT DISTINCT FROM 'lost') = (lost_reason IS NOT NULL))
);

-- The board surface: one pipeline's cards, column by column, in order.
CREATE INDEX crm_deals_by_stage ON crm_deals (tenant_id, pipeline_id, stage_id, position);
-- "My deals" and the owner filter of the list view.
CREATE INDEX crm_deals_by_owner ON crm_deals (tenant_id, owner_user_id, expected_close);
-- Every deal ever raised for one customer — the won-deal handoff to billing
-- reads this way, and so does a customer's own drawer.
CREATE INDEX crm_deals_by_customer ON crm_deals (tenant_id, customer_id)
    WHERE customer_id IS NOT NULL;

-- Append-only: what each deal did, in the transaction that did it. Funnel and
-- velocity reporting needs rows that are typed, transactional and guaranteed
-- present, which is exactly what the audit log (best-effort, free text) is
-- not — both exist and neither replaces the other (docs/design/crm.md,
-- "Moving a deal, and the history of it").
CREATE TABLE crm_deal_stage_events (
    tenant_id     TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id            TEXT NOT NULL,
    deal_id       TEXT NOT NULL,
    -- NULL on the row written when the deal was created, so "how long did this
    -- sit in Qualified" is answerable from row one rather than from row two.
    from_stage_id TEXT,
    to_stage_id   TEXT NOT NULL,
    moved_by      TEXT NOT NULL,
    moved_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, deal_id)
        REFERENCES crm_deals (tenant_id, id) ON DELETE CASCADE,
    -- A history row names the column it names, forever: deleting that column
    -- is refused (RESTRICT) rather than silently rewriting the past.
    CONSTRAINT crm_deal_stage_events_from_fk FOREIGN KEY (tenant_id, from_stage_id)
        REFERENCES crm_stages (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT crm_deal_stage_events_to_fk FOREIGN KEY (tenant_id, to_stage_id)
        REFERENCES crm_stages (tenant_id, id) ON DELETE RESTRICT
);

-- One deal's history, oldest first — the only way this table is ever read.
CREATE INDEX crm_deal_stage_events_by_deal
    ON crm_deal_stage_events (tenant_id, deal_id, moved_at, id);
-- The guard that refuses to delete a column the past has named.
CREATE INDEX crm_deal_stage_events_by_to_stage
    ON crm_deal_stage_events (tenant_id, to_stage_id);
CREATE INDEX crm_deal_stage_events_by_from_stage
    ON crm_deal_stage_events (tenant_id, from_stage_id)
    WHERE from_stage_id IS NOT NULL;
