-- What the site assistant did (migration 0328, ADR 0040, item S3.03e): a
-- tenant-facing ledger of the assistant's own acts and offers — an answer
-- with its citations, a refusal, a booking offered and a booking made, the
-- lead form offered and a lead raised. The transcript is accountability for
-- the tenant ("which facts is my bot handing out, from which pages?"), not
-- analytics.
--
-- The privacy shape is the design, and it is provable from the columns:
-- there is NO question column, NO answer-text column, NO visitor identity of
-- any kind — no token, no address, no name. `fact` names the tenant's own
-- published fact the assistant used (a service name); `citations` names the
-- tenant's own published pages an answer drew on. What a visitor typed lives
-- nowhere; who they were lives only where the act itself put it (the
-- appointment row, the CRM card) — records the tenant already owns.
--
-- The ledger is bounded: writes prune each site to its newest rows, so an
-- anonymous surface can churn the transcript but never grow it.

CREATE TABLE site_chat_actions (
    id          TEXT PRIMARY KEY,
    tenant_id   TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    site_id     TEXT NOT NULL,
    -- What the assistant did. Additive by design: later acting slices
    -- (commerce) join this list the way 'chat' joined conversion sources.
    kind        TEXT NOT NULL CHECK (kind IN (
                    'answered', 'refused', 'booking_offered', 'booked',
                    'lead_offered', 'lead_saved', 'lead_known')),
    -- The tenant-owned published fact the act used (today: the booking
    -- service's name). Never visitor input.
    fact        TEXT,
    -- The booked instant, for kind = 'booked'.
    slot_at     TIMESTAMPTZ,
    -- For 'answered': the published pages the answer drew on, as a JSON
    -- array of {"title": …, "path": …} — path null for a knowledge document,
    -- which has no public URL and is named by title alone.
    citations   JSONB,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT site_chat_actions_site_fk
        FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites(tenant_id, id) ON DELETE CASCADE
);

-- The transcript is read newest-first per site, and pruned the same way.
CREATE INDEX site_chat_actions_by_site
    ON site_chat_actions (tenant_id, site_id, occurred_at DESC, id DESC);
