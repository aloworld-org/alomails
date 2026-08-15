-- The ledger that records a tenant having been given its default agents
-- (ADR 0034 "an agent in every product"; queue item A1.5).
--
-- Until now an agent existed only because an administrator posted one to
-- `POST /chat/agents` by hand, handle included. That is a manual step between a
-- new tenant and the feature the product is sold on, and nobody performs it, so
-- the honest description of the shipped state was "agents, if you register
-- them". The set is now seeded on the first read of the agent list.
--
-- Why a ledger rather than "are there any agents yet":
--
-- "Once" has to survive what it wrote. A tenant that retires the @meet agent it
-- was given must not find it back the next morning, and the `chat_agents` rows
-- cannot answer that question once a row is gone. The primary key is also what
-- makes two simultaneous first reads produce one set without a lock: both
-- insert, exactly one wins, and the winner writes the agents.
--
-- The same shape as `inv_seeds` (migration 0157) and `fin_seeds`, deliberately:
-- a third table rather than a shared one because each block's rows are deleted
-- with their tenant through their own module's cascade, and a shared ledger
-- would make the chat seed depend on inventory's migration having run.
CREATE TABLE chat_agent_seeds (
    tenant_id  TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    system_key TEXT NOT NULL,
    seeded_by  TEXT NOT NULL,
    seeded_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, system_key)
);
