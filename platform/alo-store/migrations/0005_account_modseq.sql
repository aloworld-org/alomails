-- Account-scoped change visibility: the JMAP/IMAP state cursor becomes
-- per-account, not per-tenant. A tenant-wide modseq let co-tenant users
-- infer each other's activity *volume* (A's state token advanced when B
-- mutated, even though A never saw B's objects). IDLE builds directly on
-- this cursor, so the side channel must close before it ships. The change
-- log rows (object_changes) are already per-account (user_id, since 0004);
-- only the counter itself was shared.

CREATE TABLE account_modseq (
    tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id   TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    modseq    BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (tenant_id, user_id)
);

-- Seed each account's counter from the highest modseq already recorded
-- for it, so any existing change rows stay resumable (new_state is never
-- below a row we would return).
INSERT INTO account_modseq (tenant_id, user_id, modseq)
SELECT tenant_id, user_id, MAX(modseq)
FROM object_changes
WHERE user_id IS NOT NULL
GROUP BY tenant_id, user_id;

DROP TABLE tenant_modseq;
