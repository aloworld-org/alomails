-- The product an agent belongs to (ADR 0034).
--
-- ADR 0034 decided that every product has its own agent. What shipped was a
-- name: `chat_agents` carried a handle, a display name and a description, and
-- nothing said which product an agent was the agent *of*. So `@inventory` and
-- `@mail` were the same assistant under two names, both offered all
-- thirty-three tools, and "the Inventory agent" was a label rather than a
-- scope.
--
-- This column is the fact everything else can then be scoped by: the tool set
-- the prompt offers, the refusal at the execution boundary, and — from A1.5 —
-- which agents a tenant gets at all.
--
-- 'workspace' is "Ask alo": the one agent that is deliberately not scoped to a
-- product, because its whole job is to work across them (ADR 0034, "Ask alo is
-- itself the top-level agent"). Every other word is a product.
--
-- The accepted words are the rail's own module ids (migration 0208's
-- `tenant_user_module_denials`), plus 'mail', which has no denial row because
-- mail cannot be switched off, plus 'workspace'. Keeping the two vocabularies
-- the same is what lets A1.5 gate an agent on the module access that already
-- exists instead of inventing a second permission system. The CHECK lists
-- every product the rail has today — including the three whose tool sets are
-- still to be built (insights, meet, sites) — so a later wave adds an agent by
-- filling in a tool list, not by rewriting a constraint.

ALTER TABLE chat_agents
    ADD COLUMN product TEXT NOT NULL DEFAULT 'workspace',
    ADD CONSTRAINT chat_agents_product CHECK (product IN (
        'mail', 'agenda', 'tasks', 'chat', 'drive',
        'billing', 'crm', 'projects', 'finance', 'inventory', 'hr',
        'insights', 'meet', 'sites',
        'workspace'
    ));

-- Existing agents keep exactly the reach they have today ('workspace', every
-- tool) unless their handle already says which product they are — an agent
-- somebody named '@inventory' was created to be the Inventory agent, and the
-- handle is the only evidence of intent this table holds. An unrecognised
-- handle is left alone rather than guessed at: narrowing an agent nobody
-- described would take tools away from a working room, and widening one is
-- worse. The rest are corrected when A1.5 gives a tenant its default set.
UPDATE chat_agents
   SET product = lower(handle)
 WHERE lower(handle) IN (
        'mail', 'agenda', 'tasks', 'chat', 'drive',
        'billing', 'crm', 'projects', 'finance', 'inventory', 'hr',
        'insights', 'meet', 'sites'
    );

-- "Which agents does this tenant have for Billing?" — asked by the agent
-- directory (A3.3) and by A1.5's default set, both per tenant.
CREATE INDEX chat_agents_by_product ON chat_agents (tenant_id, product);
