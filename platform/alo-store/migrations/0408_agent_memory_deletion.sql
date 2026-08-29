-- Deletion follows the source (ADR 0057 §6, `docs/design/complete-agents.md`
-- §6, queue item A6.3). Three sources delete synchronously in the store — a
-- withdrawn message takes the facts learned from it, an archived room takes
-- its channel memories, a removed agent takes what it learned in that room.
-- The fourth source is time: a room whose memory switch has resolved OFF for
-- 30 days has its hidden memories deleted by a background sweep.

-- When the room's own memory switch last changed. The sweep dates "off since"
-- from here — and, for rooms that follow the workspace default, from
-- `agent_memory_defaults.updated_at` as well, whichever moved last. Rooms
-- that chose before this column existed start their clock now: the honest
-- reading of a moment that was never recorded.
ALTER TABLE chat_channels ADD COLUMN agent_memory_set_at TIMESTAMPTZ;
UPDATE chat_channels SET agent_memory_set_at = now() WHERE agent_memory IS NOT NULL;

-- "Deletion follows the message" finds the withdrawn message's facts by this
-- index rather than by a scan of everything the tenant's agents remember.
CREATE INDEX agent_memories_source
    ON agent_memories (tenant_id, source_msg)
    WHERE source_msg IS NOT NULL;
