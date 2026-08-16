-- The ticket email's at-most-once marker (migration 0333, ADR 0050, item
-- S3.04h).
--
-- A fulfilment row with a NULL mailed_at is a sale whose buyer has not been
-- emailed their ticket; the mail sweep claims such rows by setting mailed_at
-- in the same statement that selects them, so two concurrent sweeps cannot
-- send the same ticket twice. Expand-only: one nullable column, no rewrite.
ALTER TABLE site_ticket_fulfilments
    ADD COLUMN mailed_at TIMESTAMPTZ;

-- The sweep's read: unmailed rows whose fulfilment act has written the
-- sale's description (the mail quotes the record of the sale, so it waits
-- for it), oldest first.
CREATE INDEX site_ticket_fulfilments_unmailed
    ON site_ticket_fulfilments (created_at, id)
 WHERE mailed_at IS NULL AND description <> '';

-- The per-tenant daily ceiling's count: mails sent in the last 24 hours.
CREATE INDEX site_ticket_fulfilments_mailed_by_tenant
    ON site_ticket_fulfilments (tenant_id, mailed_at)
 WHERE mailed_at IS NOT NULL;
