-- The Docs agent's product word (ADR 0034's "Docs agent"; queue item A2.3).
--
-- The second product that is real, has an agent in ADR 0034, and has **no rail
-- app of its own** — the first was alo Sheets in migration 0404, and the
-- reasoning is that migration's, unchanged: a document is a Drive node
-- (`kind = 'doc'`) whose blob is the block tree the editor writes (ADR 0031),
-- opened from Drive, so there is no 'docs' row in `tenant_user_module_denials`
-- and there should not be one. The switch that decides whether somebody may
-- open a document is Drive's, and a second switch could disagree with the first
-- about the same file.
--
-- So the word is added here and the gate translates it: `chat_agents`'
-- visibility predicate compares a denial against 'drive' when the agent's
-- product is 'docs' as well as when it is 'sheets' (see `AGENT_GATE`, held to
-- `AgentProduct::module` by a test). Without that translation this word would
-- be an agent no denial could ever hide.
--
-- Widening only: every word 0404 accepted is still accepted, and no row can
-- fail the new constraint because none can hold 'docs' yet.

ALTER TABLE chat_agents
    DROP CONSTRAINT chat_agents_product,
    ADD CONSTRAINT chat_agents_product CHECK (product IN (
        'mail', 'agenda', 'tasks', 'chat', 'drive', 'sheets', 'docs',
        'billing', 'crm', 'projects', 'finance', 'inventory', 'hr',
        'insights', 'meet', 'sites',
        'workspace'
    ));

-- An agent an administrator had already registered as '@docs' was created to be
-- the Docs agent, exactly as 0401 and 0404 read the other handles. Nothing else
-- is touched: a tenant seeded before this migration is offered the agent once,
-- under its own ledger key (`LATER_AGENT_PRODUCTS` in `chat_agent_seed`), which
-- keeps "a retired agent stays retired" true for it as well.
UPDATE chat_agents SET product = 'docs' WHERE lower(handle) = 'docs';
