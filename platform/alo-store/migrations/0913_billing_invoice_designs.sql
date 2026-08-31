-- The presentation snapshot of an invoice. It is editable while the invoice
-- is a draft and frozen with the legal document when that invoice is issued.
CREATE TABLE billing_invoice_designs (
    tenant_id   TEXT NOT NULL,
    invoice_id  TEXT NOT NULL,
    design      JSONB NOT NULL,
    updated_by  TEXT NOT NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, invoice_id),
    FOREIGN KEY (tenant_id, invoice_id)
        REFERENCES billing_invoices (tenant_id, id) ON DELETE CASCADE
);
