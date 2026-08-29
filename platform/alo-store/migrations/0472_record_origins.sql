-- Provenance on records (ADR 0058 §4, `docs/design/complete-agents.md` §2,
-- queue item A4.5): a record carries where it came from — the thread a task
-- came from, the quote an invoice came from, the meeting a decision came
-- from — so an agent explains rather than asserts, and every answer links
-- into the record.
--
-- One table rather than a column per module: provenance is one concern with
-- one shape (`origin = {kind, id, label}`), it is set once at creation and
-- never updated, and the modules whose records carry it live in tables this
-- track reads but does not restructure. The record view joins this row in
-- and shows `origin` as a field on the record, which is what the design
-- names.
--
-- The PRIMARY KEY makes "set once" structural: the first writer wins
-- (INSERT ... ON CONFLICT DO NOTHING in the store), because where a record
-- came from is a fact about its creation, not a field anybody edits later.
-- No foreign keys to the record or the origin, matching `events` (0406) and
-- the action record (0470): a provenance row is a pointer, and it outlives
-- what it points at rather than constraining every module's delete.
CREATE TABLE record_origins (
    tenant_id    TEXT NOT NULL,
    -- The record word the module's own replies use (`invoice`, `task`,
    -- `deal`) and the record's id — same vocabulary as `events` (0406).
    record_type  TEXT NOT NULL,
    record_id    TEXT NOT NULL,
    -- Where it came from: the source's kind (`quote`, `thread`, `meeting`,
    -- `email`), its id, and the label a person would cite it by ("the
    -- Northstar thread", "QUO-2026-00007"). The label is NULL when the
    -- source has no name of its own (a bare DM) — the pointer still stands.
    origin_kind  TEXT NOT NULL,
    origin_id    TEXT NOT NULL,
    origin_label TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, record_type, record_id)
);
