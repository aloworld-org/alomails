-- Each mailbox gets a durable identity of its own (ADR 0051, stage 8).
--
-- WHY. A MAPI client identifies an object by a namespace and a counter. The
-- namespace is the store's GUID: it appears in the logon response, in every
-- PidTagSourceKey and PidTagChangeKey, and as the REPLGUID of every IDSET a
-- client sends back. alo has been answering all of those with sixteen zero
-- bytes, which was harmless while a client could only read — nothing compared
-- one mailbox's identifiers with another's.
--
-- Incremental synchronization compares constantly, and a shared namespace stops
-- being harmless the moment one Outlook profile holds two alo accounts. Their
-- identifiers would be drawn from the same namespace and the same small counter
-- range, so the same 22 bytes would name a different message in each. The
-- client has no way to notice: it would file one account's mail against the
-- other's identifiers and neither would ever converge.
--
-- WHY HERE, on the id allocator's row. The GUID and the counter are two halves
-- of one identifier and are only ever meaningful together. Keeping them in one
-- row means a mailbox cannot acquire a counter space without also acquiring the
-- namespace that space belongs to, which is a thing a second table would let
-- happen.
--
-- WHY A DEFAULT AND NOT AN APPLICATION VALUE. gen_random_uuid() is in core
-- Postgres from 13 onward, so the database can promise this itself. A value the
-- application supplies could be forgotten on one insert path; a default cannot.
--
-- Expand-only: one column with a default on a table that is new in the release
-- before this one, so no existing row needs a value invented for it.
ALTER TABLE mapi_id_counter
    ADD COLUMN replica_guid UUID NOT NULL DEFAULT gen_random_uuid();

COMMENT ON COLUMN mapi_id_counter.replica_guid IS
    'The store GUID this mailbox presents to MAPI clients (ADR 0051). Stable '
    'for the life of the account: a client caches it inside every identifier '
    'it holds, so changing it invalidates that client''s whole replica.';

-- Two mailboxes sharing a namespace is the failure this column exists to
-- prevent, so it is refused rather than trusted to the generator.
CREATE UNIQUE INDEX mapi_id_counter_replica_guid_unique
    ON mapi_id_counter (replica_guid);
