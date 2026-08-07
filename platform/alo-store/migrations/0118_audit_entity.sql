-- The audit log learns which record an entry is about (ADR 0035, wave B2.13).
--
-- `audit_log` (0015) was built for administrative actions, where the target is
-- a name a person recognises — a domain, a user. The business modules need the
-- other reading of the same question: not "what happened in this tenant" but
-- "what happened to THIS invoice", asked from the record itself. That needs the
-- subject of an entry to be a machine-addressable pair rather than a label, so
-- an entry can be looked up by the record it belongs to instead of scanned for.
--
-- `entity_type` is a stable dotted name for the kind of record
-- (`billing.invoice`, `crm.deal`), `entity_id` the record's own id within this
-- tenant. Both nullable and no backfill: every existing row is an
-- administrative action about a target that is not one of these records, and
-- NULL says exactly that. Expand-only — nothing reads these columns until the
-- code that writes them ships, and `target`/`detail` keep their meaning.
--
-- The log stays append-only: there is no UPDATE or DELETE path to `audit_log`
-- anywhere in the codebase, and rows leave only with the tenant they belong to
-- (the 0015 `ON DELETE CASCADE`). That is a property of the code, so it is
-- tested rather than asserted here — a database-level guarantee would need a
-- role split this deployment does not have yet.
ALTER TABLE audit_log ADD COLUMN entity_type TEXT;
ALTER TABLE audit_log ADD COLUMN entity_id TEXT;

-- Read pattern: one record's history, newest first. Partial, because the vast
-- majority of an old tenant's rows are administrative and would only make the
-- index bigger without ever being read through it.
CREATE INDEX audit_log_entity ON audit_log (tenant_id, entity_type, entity_id, created_at DESC)
    WHERE entity_type IS NOT NULL;
