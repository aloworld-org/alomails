-- alo Billing (ADR 0035, wave B1.19): money received against an invoice.
--
-- A payment is a **fact that happened**, not an opinion about a document: a
-- date, an amount, how it arrived and the bank's own reference. There may be
-- many against one invoice — a customer paying a large bill in instalments is
-- ordinary B2B behaviour — which is why this is a table and not a column.
--
-- **The invoice's paid-state is derived from these rows, never independently
-- written.** `billing_invoices.status` is a projection the store recomputes
-- inside the same transaction that inserts or removes a payment (with the
-- invoice row locked), so the column can never disagree with the ledger
-- underneath it; no request can set it. "Partially paid" is deliberately NOT a
-- status value: it is a fact about money (sum of payments against gross), not a
-- state of the document, and putting it in the status column would mean the
-- four legal document states suddenly had a fifth that a tax authority has
-- never heard of.
--
-- Amounts are integer cents and strictly positive. A payment that "un-pays"
-- an invoice is not a negative payment — it is the removal of a payment that
-- was recorded wrongly, or a credit note (B1.09) if the debt itself changed.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE billing_payments (
    tenant_id    TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id           TEXT NOT NULL,
    -- The document settled. The composite reference pins the invoice to the
    -- SAME tenant at the database level, so even a bug in a WHERE clause
    -- cannot attach one tenant's money to another tenant's document. The
    -- cascade only ever fires for a draft (the only invoice that is deleted),
    -- and a draft cannot carry payments in the first place.
    invoice_id   TEXT NOT NULL,
    -- The day the money arrived, as the bank states it — not the day it was
    -- keyed in, which is `created_at`. The two differ routinely, and the VAT
    -- report (B1.20) and the ledger (B4) both need the former.
    paid_on      DATE NOT NULL,
    -- Strictly positive integer cents, in the document's own currency (the
    -- invoice carries it; a payment in another currency is B1.21's problem and
    -- is not representable here on purpose).
    amount_cents BIGINT NOT NULL,
    -- How it arrived, free text bounded by the store: 'bank transfer', 'SEPA
    -- direct debit', 'card', 'cash'. Deliberately not an enum — the set varies
    -- per member state and per tenant, and B4 maps methods to ledger accounts
    -- with a per-tenant table rather than a hardcoded list.
    method       TEXT NOT NULL DEFAULT '',
    -- The bank's own reference (end-to-end id, statement line), which is what
    -- reconciliation (B4.09) matches on.
    reference    TEXT NOT NULL DEFAULT '',
    created_by   TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, invoice_id)
        REFERENCES billing_invoices (tenant_id, id) ON DELETE CASCADE,
    -- Defence in depth: the store validates this before writing, so a
    -- violation here means a bug in our code, not bad user input.
    CONSTRAINT billing_payments_amount_positive
        CHECK (amount_cents > 0 AND amount_cents <= 1000000000000)
);

-- The payment ledger of one document, newest first — the read behind the
-- invoice's payments panel and behind every settlement sum.
CREATE INDEX billing_payments_by_invoice
    ON billing_payments (tenant_id, invoice_id, paid_on DESC, id);
