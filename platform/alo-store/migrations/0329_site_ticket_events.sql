-- Tickets and dated products, wave one of alo Commerce (migration 0329,
-- ADR 0041, item S3.04b): the event a site sells seats to, and the hold that
-- makes overselling impossible.
--
-- The shop is a surface, not a system: the event stores a *reference* into
-- Billing's price list (product_id) and never a copy of the name, the price
-- or the VAT rate — those are asked of Billing's catalog seam at render and
-- at sale, so there is never a second number to reconcile. What IS new data,
-- and therefore lives here, is the seat count: capacity is the one fact
-- neither Billing nor Agenda holds.
--
-- The hold is the concurrency primitive the ADR calls "the first commit, not
-- hardening": capacity is reserved BEFORE payment and released if the buyer
-- does not finish. A hold in state 'held' counts against capacity only while
-- expires_at is in the future, so expiry frees seats by time passing — no
-- sweeper is needed for correctness, and there is no instant in which an
-- abandoned checkout blocks a live buyer.
--
-- Privacy is provable from the columns: a hold has NO buyer identity of any
-- kind — no name, no address, no token. Who bought lives where the sale puts
-- it (the order, the invoice, the CRM card — S3.04c/d), in records the
-- tenant already owns. A hold is pure seat accounting.

CREATE TABLE site_ticket_events (
    id          TEXT PRIMARY KEY,
    tenant_id   TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    site_id     TEXT NOT NULL,
    -- The reference into Billing's price list. Never a copied price: the
    -- catalog seam (billing_catalog_read) answers what this costs *now*.
    product_id  TEXT NOT NULL,
    starts_at   TIMESTAMPTZ NOT NULL,
    capacity    INTEGER NOT NULL CHECK (capacity >= 1 AND capacity <= 100000),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT site_ticket_events_site_fk
        FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites(tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT site_ticket_events_tenant_scoped UNIQUE (tenant_id, id)
);

CREATE INDEX site_ticket_events_by_site
    ON site_ticket_events (tenant_id, site_id, starts_at, id);

CREATE TABLE site_ticket_holds (
    id           TEXT PRIMARY KEY,
    tenant_id    TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    site_id      TEXT NOT NULL,
    event_id     TEXT NOT NULL,
    quantity     INTEGER NOT NULL CHECK (quantity >= 1),
    -- 'held' counts against capacity only while expires_at > now(); the
    -- other three states are terminal. 'completed' is a seat sold (S3.04c
    -- settles payment onto it), 'released' a buyer who walked away,
    -- 'expired' a hold a sweep has tidied — but a stale 'held' row past its
    -- expiry already counts for nothing and reads back as expired.
    state        TEXT NOT NULL DEFAULT 'held'
                 CHECK (state IN ('held', 'completed', 'released', 'expired')),
    expires_at   TIMESTAMPTZ NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    CONSTRAINT site_ticket_holds_event_fk
        FOREIGN KEY (tenant_id, event_id)
        REFERENCES site_ticket_events(tenant_id, id) ON DELETE CASCADE
);

-- The seat count under the advisory lock, and the availability read.
CREATE INDEX site_ticket_holds_by_event
    ON site_ticket_holds (tenant_id, event_id, state, expires_at);
