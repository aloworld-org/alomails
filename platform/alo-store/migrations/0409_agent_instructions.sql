-- Standing instructions (ADR 0057, `docs/design/complete-agents.md` §7,
-- queue item A7.1). A person asks once, in advance: a card in the channel
-- with the instruction in the author's words, the trigger — a schedule, or a
-- module event the intent registry names — and Cancel for the author and the
-- room owner. Each firing is a turn with the author as asker: reads post
-- into the room, writes propose to the author.

CREATE TABLE agent_instructions (
    tenant_id      TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    id             TEXT NOT NULL,
    agent_id       TEXT NOT NULL,
    channel_id     TEXT NOT NULL,
    author_id      TEXT NOT NULL,
    -- The instruction in the author's words — each firing runs it verbatim as
    -- the turn's question, so the card can never drift from what actually runs.
    text           TEXT NOT NULL,
    trigger_kind   TEXT NOT NULL,
    -- 'event': fires when the tenant's stream gains an event of this kind
    -- (a verb the intent registry names), coalescing everything since the
    -- last firing into one turn.
    event_kind     TEXT,
    -- 'schedule': the repeat. At least hourly — the "one firing per
    -- instruction per hour" bound as a property of the schema rather than a
    -- hope about the sweeper.
    repeat_minutes INTEGER,
    next_run       TIMESTAMPTZ,
    last_fired_at  TIMESTAMPTZ,
    -- Set when the author leaves the room ("paused, the card says so").
    -- Nothing unpauses it in v1; the author re-creates the instruction.
    paused_at      TIMESTAMPTZ,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    CONSTRAINT agent_instructions_trigger CHECK (trigger_kind IN ('schedule', 'event')),
    -- The two trigger shapes are exclusive rows: a schedule has its repeat
    -- and its clock and no event; an event trigger has its kind and neither.
    CONSTRAINT agent_instructions_shape CHECK (
        (trigger_kind = 'schedule' AND repeat_minutes IS NOT NULL
             AND next_run IS NOT NULL AND event_kind IS NULL)
        OR (trigger_kind = 'event' AND event_kind IS NOT NULL
             AND repeat_minutes IS NULL AND next_run IS NULL)
    ),
    CONSTRAINT agent_instructions_hourly CHECK (repeat_minutes IS NULL OR repeat_minutes >= 60)
);

-- The card lists a room's instructions; the twenty-per-channel cap counts them.
CREATE INDEX agent_instructions_channel ON agent_instructions (tenant_id, channel_id);

-- The sweep's due scan for schedules.
CREATE INDEX agent_instructions_due ON agent_instructions (next_run)
    WHERE trigger_kind = 'schedule' AND paused_at IS NULL;

-- An event trigger asks "any event of this kind since I last fired" —
-- answered by this index rather than a scan of the tenant's whole stream.
CREATE INDEX events_kind_created ON events (tenant_id, kind, created_at);
