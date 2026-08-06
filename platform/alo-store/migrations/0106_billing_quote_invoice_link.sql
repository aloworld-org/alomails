-- alo Billing (ADR 0035, wave B1): the invoice an accepted quote produces.
--
-- Accepting an offer is the moment it stops being an offer and starts being
-- work that will be billed. B1.12 makes that one act: the quote closes as
-- `accepted` and a DRAFT invoice carrying a copy of its lines appears, in the
-- same transaction. This migration adds the link from that invoice back to the
-- quote it came from — the direction that answers both questions a user asks
-- ("where did this invoice come from?" from the invoice, "was this offer
-- billed?" from the quote, through the unique index below).
--
-- WHY THE COLUMN LIVES ON THE INVOICE, not a `invoice_id` column on the quote:
-- the invoice is the newer document and the one that knows its own origin. A
-- column on the quote would have to be written into a row that is frozen the
-- moment it is sent, which is exactly the property that makes a sent quote
-- trustworthy.
--
-- Expand-only: one nullable column, one CHECK no existing row can violate
-- (nothing has ever been written with a quote link), one index. Nothing is
-- dropped or rewritten.

ALTER TABLE billing_invoices
    ADD COLUMN quote_id TEXT;

-- The composite reference pins the quote to the SAME tenant at the database
-- level, exactly as `customer_id` is pinned: even a bug in a WHERE clause
-- cannot link an invoice to another tenant's offer.
--
-- NO ACTION (the default) rather than CASCADE or SET NULL: only a DRAFT quote
-- is ever deleted, and a draft has never been accepted, so no linked quote can
-- be removed while the invoice stands. CASCADE would be actively wrong — it
-- would delete an invoice, which is the one thing this module never does — and
-- dropping a tenant still works, because NO ACTION is checked after the whole
-- cascade rather than row by row.
ALTER TABLE billing_invoices
    ADD CONSTRAINT billing_invoices_quote_fk
    FOREIGN KEY (tenant_id, quote_id)
        REFERENCES billing_quotes (tenant_id, id);

-- A credit note is raised against an invoice, never against a quote: it copies
-- the document it credits, and that document's own origin is not part of what
-- it reverses. The store never writes it; this is the database saying so too.
ALTER TABLE billing_invoices
    ADD CONSTRAINT billing_invoices_credit_note_has_no_quote
    CHECK (NOT (is_credit_note AND quote_id IS NOT NULL));

-- One invoice per accepted quote, ever. Acceptance is a terminal transition
-- (`accepted` has no successor), so the store can produce at most one — and
-- this index is what makes "the invoice raised from this offer" a single row a
-- reader can rely on rather than a list it has to disambiguate. Postgres
-- allows many NULLs in a unique index, so the overwhelming majority of
-- invoices — which come from no quote at all — never collide.
CREATE UNIQUE INDEX billing_invoices_from_quote
    ON billing_invoices (tenant_id, quote_id)
    WHERE quote_id IS NOT NULL;
