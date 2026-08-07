-- alo CRM (ADR 0035, wave B2): what was said and done on a deal.
--
-- A note, a logged call, a meeting — written once, never edited (a correction
-- is another note, which is what a log of what was said ought to be) and
-- deleted only by the colleague who wrote it (docs/design/crm.md, "Activities
-- and next steps"). The row is readable tenant-wide like the deal it hangs on.
--
-- A NEXT STEP IS NOT A ROW HERE. It is a real task in the tasks module, carried
-- by the source link ADR 0021 already publishes (`tasks.source_kind = 'deal'`,
-- `tasks.source_id = <deal id>`) — the additive third value beside 'email' and
-- 'event'. Two to-do lists in one workspace is how a CRM becomes the system
-- nobody updates, so this table deliberately has no `due_at` and no `done`.
--
-- `happened_at` is when it happened, which is not when it was written: a call
-- logged an hour later is dated the hour it took place, and the drawer reads by
-- that column. `created_at` is kept beside it so the record still knows when it
-- was actually entered.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE crm_activities (
    tenant_id      TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id             TEXT NOT NULL,
    deal_id        TEXT NOT NULL,
    -- 'note' | 'call' | 'meeting'. A closed vocabulary, because it is what the
    -- drawer renders an icon from and a report will count by; free text here
    -- would be three spellings of "call" within a month.
    kind           TEXT NOT NULL DEFAULT 'note',
    -- What was said. Required — an empty note records nothing.
    body           TEXT NOT NULL,
    -- When it happened (defaults to when it was written).
    happened_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Who wrote it: the only colleague who may delete it again.
    author_user_id TEXT NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- The activity is the deal's own note; deleting the deal takes it with it,
    -- and within the tenant always.
    FOREIGN KEY (tenant_id, deal_id)
        REFERENCES crm_deals (tenant_id, id) ON DELETE CASCADE,
    -- Defence in depth: the store validates both before writing, so a
    -- violation here means a bug in our code rather than bad user input.
    CONSTRAINT crm_activities_kind_known
        CHECK (kind IN ('note', 'call', 'meeting')),
    CONSTRAINT crm_activities_body_shape CHECK (length(btrim(body)) > 0)
);

-- The deal drawer's read: this deal's log, most recent first. Also the count
-- the per-deal cap is enforced with, under the deal's row lock.
CREATE INDEX crm_activities_by_deal
    ON crm_activities (tenant_id, deal_id, happened_at DESC, id DESC);

-- "The next steps of this deal": the tasks module's source link, read back the
-- other way round. Without it, a deal drawer scans every task of the tenant.
CREATE INDEX tasks_by_source ON tasks (tenant_id, source_kind, source_id)
    WHERE source_kind IS NOT NULL AND source_id IS NOT NULL;
