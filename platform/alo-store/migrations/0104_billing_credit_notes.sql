-- alo Billing (ADR 0035, wave B1): credit notes.
--
-- A credit note is not a new table. It is an invoice with negated lines that
-- names the document it credits — `is_credit_note` and `credits_invoice_id`
-- have been on `billing_invoices` since 0102, together with the CHECK that ties
-- them to each other and the composite foreign key that keeps the credited
-- document inside the same tenant. B1.09 fills them in, and adds the two things
-- the relation needs to be trustworthy and readable.
--
-- Expand-only: one CHECK that no existing row can violate (no credit note has
-- ever been written) and one index. Nothing is dropped or rewritten.

-- A document cannot credit itself. That would be a one-row cycle: every walk of
-- the credit chain (the ledger view, the print view, a future audit export)
-- would either loop or have to defend itself against a state the business can
-- never be in. The store never writes it; this is the database saying so too.
ALTER TABLE billing_invoices
    ADD CONSTRAINT billing_invoices_credit_note_is_not_itself
    CHECK (credits_invoice_id IS DISTINCT FROM id);

-- "What credits this invoice?" — the read behind the ledger of an original and
-- its credit notes. Partial, because only credit notes carry the column and
-- ordinary invoices are the overwhelming majority of the table.
CREATE INDEX billing_invoices_credits
    ON billing_invoices (tenant_id, credits_invoice_id)
    WHERE credits_invoice_id IS NOT NULL;
