-- The ticket fulfilment — what one paid ticket order produced (migration
-- 0331, ADR 0041, item S3.04d).
--
-- The order (0330) is the sale; the fulfilment is the record of the sale
-- being made good: the ticket the buyer can hold (the token — a capability,
-- like a booking's manage_token), the invoice Billing raised for it and the
-- CRM outcome, each written by the background sweep through the owning
-- module's own door. One paid order is fulfilled exactly once
-- (site_ticket_fulfilments_one_per_order): the sweep claims by inserting
-- this row, so two concurrent sweeps cannot fulfil the same sale twice.
--
-- Deliberately NO buyer column of any kind: who bought lives on the order,
-- and the ticket page reads it through the order join. `description` is not
-- a copy of the price list — it is the record of what was sold, written once
-- at fulfilment, the same way the order records the price that was struck.

CREATE TABLE site_ticket_fulfilments (
    id             TEXT PRIMARY KEY,
    tenant_id      TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    site_id        TEXT NOT NULL,
    order_id       TEXT NOT NULL,
    event_id       TEXT NOT NULL,
    -- The ticket itself: the capability the buyer holds. Globally unique so
    -- the public ticket page can refuse strangers with one uniform absence.
    token          TEXT NOT NULL,
    -- What was sold, as the invoice line said it: "<product> — <date>".
    description    TEXT NOT NULL DEFAULT '',
    -- Billing's document for this sale, when one could be raised; when not,
    -- invoice_note says why in a sentence the tenant can act on.
    invoice_id     TEXT,
    invoice_number TEXT,
    invoice_note   TEXT,
    -- What CRM answered: a raised lead, or the fact that made one
    -- unnecessary (already-known, already-customer).
    crm_outcome    TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT site_ticket_fulfilments_site_fk
        FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites(tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT site_ticket_fulfilments_order_fk
        FOREIGN KEY (tenant_id, order_id)
        REFERENCES site_ticket_orders(tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT site_ticket_fulfilments_one_per_order UNIQUE (tenant_id, order_id),
    CONSTRAINT site_ticket_fulfilments_tenant_scoped UNIQUE (tenant_id, id),
    CONSTRAINT site_ticket_fulfilments_token UNIQUE (token)
);

CREATE INDEX site_ticket_fulfilments_by_site
    ON site_ticket_fulfilments (tenant_id, site_id, created_at DESC, id);
