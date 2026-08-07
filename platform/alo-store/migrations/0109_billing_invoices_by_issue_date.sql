-- alo Billing (ADR 0035, wave B1.20): the index the VAT summary reads on.
--
-- The report asks one question — "every document that stands, issued between
-- these two days" — and the existing indexes cannot answer it: they are keyed
-- on `created_at` (the day a draft was keyed in) rather than on `issue_date`
-- (the day the document was numbered, which is the tax point). Without this,
-- a quarter's summary scans every invoice the tenant has ever raised.
--
-- `issue_date` is NULL on exactly the drafts, which the report excludes, so the
-- index is partial: it holds only the rows the report can return, and an
-- abandoned draft costs nothing to keep.
--
-- Additive and expand-only: no column changes, no data moves.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE INDEX billing_invoices_by_issue_date
    ON billing_invoices (tenant_id, issue_date, status)
    WHERE issue_date IS NOT NULL;
