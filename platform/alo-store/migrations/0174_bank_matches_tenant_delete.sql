-- alo Finance (wave B7.02): a tenant who has reconciled a bank line can still
-- be deleted (docs/design/finance.md, "As built (B4.03a)" — the same lesson,
-- two more keys).
--
-- Migration 0143 gave `bank_matches` two `ON DELETE RESTRICT` keys, to
-- `billing_payments` and `fin_entries`: a line must never claim to be settled
-- by a payment that is gone, and money in the books must never lose the entry
-- that explains it. Both rules stand. What RESTRICT gets wrong is *when* it
-- asks: it is checked immediately, in the middle of the statement, so
-- `DELETE FROM tenants` fails the moment its cascade reaches a payment while
-- the match naming that payment — doomed by its own cascade through
-- `bank_statements` → `bank_lines` — has not been reached yet. Tenant deletion
-- is the erasure path (GDPR, not housekeeping), and it must not depend on
-- which cascade Postgres happens to run first.
--
-- `NO ACTION` (the default) is the same rule asked at the end of the
-- statement: deleting a matched payment on its own still fails with SQLSTATE
-- 23503 — which `delete_billing_payment` maps to a refusal naming the way out
-- (take the match back first). `fin_postings` → `fin_accounts` made exactly
-- this move in 0131 after 0106 taught it; these two keys were written later
-- and missed the lesson.
--
-- NO ACTION alone is NOT enough for erasure, though, and 0131's precedent
-- does not carry: `fin_postings` hangs off `tenants` directly, so its cascade
-- is queued with the others and runs before the end-of-statement check —
-- while `bank_matches` hangs two hops away, and Postgres fires queued
-- foreign-key events in order, so the check on a deleted payment can run
-- before the two-hop cascade reaches the match that names it. The erasure
-- path (`delete_tenant`) therefore clears `bank_matches` itself, first, in
-- the same transaction; this migration removes the mid-statement refusal so
-- that clearing is sufficient.
--
-- The line key stays CASCADE: a match is a fact about its line, and deleting
-- an import takes its matches with it (0143's own words).

ALTER TABLE bank_matches
    DROP CONSTRAINT bank_matches_tenant_id_payment_id_fkey,
    ADD CONSTRAINT bank_matches_tenant_id_payment_id_fkey
        FOREIGN KEY (tenant_id, payment_id)
        REFERENCES billing_payments (tenant_id, id) ON DELETE NO ACTION,
    DROP CONSTRAINT bank_matches_tenant_id_entry_id_fkey,
    ADD CONSTRAINT bank_matches_tenant_id_entry_id_fkey
        FOREIGN KEY (tenant_id, entry_id)
        REFERENCES fin_entries (tenant_id, id) ON DELETE NO ACTION;
