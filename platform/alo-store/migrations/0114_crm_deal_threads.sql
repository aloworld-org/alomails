-- alo CRM (ADR 0035, wave B2): the conversations a deal belongs to.
--
-- This is the module's reason to exist — a deal does not need a plugin to see
-- the conversation it came from, because the conversation and the deal are rows
-- in the same database under the same tenant — and it is also the sharpest
-- privacy question in the module, so the table is deliberately thin.
--
-- A link says exactly one thing: *this deal and this conversation belong
-- together*. It stores the thread's id, who linked it, and when. It stores NO
-- message content — not a body, not a participant list, not a count
-- (docs/design/crm.md, "Deal <-> mail thread"). Mail stays in mail: every read
-- of a linked conversation resolves through the READING user's own account
-- door, because `messages.user_id` is per user while a deal is tenant-wide.
--
-- Writing a link requires the thread to resolve through the LINKER's own door
-- (alo_store::crm_deal_threads::link_crm_deal_thread), so a user cannot attach a
-- conversation they have never seen by guessing an id. That rule is in code
-- because it is a per-user rule; the composite foreign key below is the
-- database's own backstop for the coarser one — a thread of ANOTHER TENANT can
-- never be stored here at all.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

-- `threads` is keyed on its id alone (0001_initial_schema.sql), which is enough
-- to point at a thread but not enough to point at a thread OF THIS TENANT. This
-- unique index — trivially satisfied, since `id` is already the primary key —
-- is what lets the foreign key below carry `tenant_id` through, so the tenancy
-- of a link is enforced by the database and not only by our code.
CREATE UNIQUE INDEX threads_tenant_id_key ON threads (tenant_id, id);

CREATE TABLE crm_deal_threads (
    tenant_id TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    deal_id   TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    -- The user who confirmed the link. Where a reader cannot open the
    -- conversation, this is the useful answer the UI shows: "ask Sam".
    linked_by TEXT NOT NULL,
    linked_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- One row per (deal, conversation): linking twice is the same link, which is
    -- what makes the route idempotent rather than an error a user has to read.
    PRIMARY KEY (tenant_id, deal_id, thread_id),
    -- The link is the deal's own note; deleting the deal takes it with it.
    FOREIGN KEY (tenant_id, deal_id)
        REFERENCES crm_deals (tenant_id, id) ON DELETE CASCADE,
    -- Within the tenant, always: a deal may only ever name a conversation of its
    -- own tenant, and a conversation that is destroyed leaves no dangling link.
    CONSTRAINT crm_deal_threads_thread_fk FOREIGN KEY (tenant_id, thread_id)
        REFERENCES threads (tenant_id, id) ON DELETE CASCADE
);

-- The deal drawer's read: this deal's conversations, most recently linked first.
CREATE INDEX crm_deal_threads_by_deal
    ON crm_deal_threads (tenant_id, deal_id, linked_at DESC);
-- The other direction — "which deals does this conversation belong to" — which
-- the mail surface asks, and which the suggestion read uses to leave out the
-- conversations a deal already holds.
CREATE INDEX crm_deal_threads_by_thread
    ON crm_deal_threads (tenant_id, thread_id);
