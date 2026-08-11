-- alo Sites S2.04a: version history keeps its shape when an older version is
-- put back online. Restoring never rewrites or re-points history: it appends a
-- NEW publish holding a copy of the chosen one, so every publish keeps exactly
-- one identity (the public cache key is `<publish_id>:<path>`) and the record
-- of what happened survives. `restored_from` names the publish that was copied
-- and is NULL for an ordinary publish.
--
-- Expand-only: one nullable column, no rewrite of existing rows.

ALTER TABLE site_publishes ADD COLUMN restored_from TEXT;

-- Composite FK, same shape as the published-set pointer: provenance can only
-- ever name a publish of the same tenant. No referential action — publishes
-- die only with their site, which removes both rows in one statement.
ALTER TABLE site_publishes ADD CONSTRAINT site_publishes_restored_from_fk
    FOREIGN KEY (tenant_id, restored_from)
    REFERENCES site_publishes (tenant_id, id);
