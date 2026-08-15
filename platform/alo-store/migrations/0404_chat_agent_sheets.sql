-- The Sheet agent's product word (ADR 0034's "Sheet agent"; queue item A2.2).
--
-- Migration 0401 set the accepted products to the rail's own module ids plus
-- 'mail' and 'workspace', on the reasoning that keeping the two vocabularies
-- identical is what lets the module gate double as the agent gate. alo Sheets
-- is the first product that is real, has an agent in ADR 0034, and has **no
-- rail app of its own**: a spreadsheet is a Drive node (`kind = 'sheet'`),
-- opened from Drive, so there is no 'sheets' row in `tenant_user_module_denials`
-- and there should not be one — the switch that decides whether somebody may
-- open a spreadsheet is Drive's, and a second switch could disagree with the
-- first about the same file.
--
-- So the word is added here and the gate translates it: `chat_agents`'
-- visibility predicate compares a denial against 'drive' when the agent's
-- product is 'sheets' (see `AGENT_GATE`, held to `AgentProduct::module` by a
-- test). Without that translation this word would be an agent no denial could
-- ever hide.
--
-- Widening only: every word 0401 accepted is still accepted, and no row can
-- fail the new constraint because none can hold 'sheets' yet.

ALTER TABLE chat_agents
    DROP CONSTRAINT chat_agents_product,
    ADD CONSTRAINT chat_agents_product CHECK (product IN (
        'mail', 'agenda', 'tasks', 'chat', 'drive', 'sheets',
        'billing', 'crm', 'projects', 'finance', 'inventory', 'hr',
        'insights', 'meet', 'sites',
        'workspace'
    ));

-- An agent an administrator had already registered as '@sheets' was created to
-- be the Sheet agent, exactly as 0401 read the other handles. Nothing else is
-- touched: a tenant seeded before this migration is offered the agent once,
-- under its own ledger key (`LATER_PRODUCTS` in `chat_agent_seed`), which keeps
-- "a retired agent stays retired" true for it as well.
UPDATE chat_agents SET product = 'sheets' WHERE lower(handle) = 'sheets';
