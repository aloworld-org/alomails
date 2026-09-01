-- Explicit Billing provenance for documents raised from Sales opportunities.
-- Exactly one document id is present; composite foreign keys preserve tenant
-- isolation and remove the relationship when an unissued draft is deleted.
CREATE TABLE crm_deal_billing_documents (
    tenant_id  TEXT NOT NULL,
    deal_id    TEXT NOT NULL,
    quote_id   TEXT,
    invoice_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    FOREIGN KEY (tenant_id, deal_id)
        REFERENCES crm_deals (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, quote_id)
        REFERENCES billing_quotes (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, invoice_id)
        REFERENCES billing_invoices (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT crm_deal_billing_documents_one_document CHECK (
        (quote_id IS NOT NULL)::integer + (invoice_id IS NOT NULL)::integer = 1
    )
);

CREATE UNIQUE INDEX crm_deal_billing_documents_quote
    ON crm_deal_billing_documents (tenant_id, quote_id)
    WHERE quote_id IS NOT NULL;
CREATE UNIQUE INDEX crm_deal_billing_documents_invoice
    ON crm_deal_billing_documents (tenant_id, invoice_id)
    WHERE invoice_id IS NOT NULL;
CREATE INDEX crm_deal_billing_documents_by_deal
    ON crm_deal_billing_documents (tenant_id, deal_id, created_at DESC);
