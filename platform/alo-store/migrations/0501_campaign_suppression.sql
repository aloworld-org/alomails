-- Who this tenant may never mail again (alo Campaigns, ADR 0044 §2; queue item
-- C1.3).
--
-- ADR 0044 §2: "suppression is absolute and global to the tenant. An
-- unsubscribe, a hard bounce or a complaint removes somebody from every future
-- send, and no segment, import or re-upload can bring them back."
--
-- Two words in that sentence decide this whole table.
--
-- ABSOLUTE. There is no `lifted_at`, no `active` flag and no delete path in the
-- store above this file. A row here is a fact that only ever accumulates, and
-- the audience's recipients query excludes it in SQL rather than leaving the
-- exclusion to whoever writes the sender. A rule the sender applies is a rule
-- the next sender forgets, and the failure lands in somebody's inbox rather
-- than in a log.
--
-- GLOBAL TO THE TENANT. Keyed by (tenant_id, address) and nothing else: not by
-- customer, not by deal, not by campaign. The same person is a billing
-- customer, the contact on two deals and a form submitter at once (ADR 0044's
-- "there is no list"), so an unsubscribe held against any one of those records
-- would suppress one copy of them and mail the other two.
--
-- ONE ROW PER ADDRESS, AND THE FIRST REASON STANDS. Unlike `campaign_consent`,
-- which is a table of events because "how do we know they agreed" needs every
-- statement ever given, this is a table of state: the only question the
-- audience asks is "is this address suppressed", and that question must have
-- one answer. A second suppression of an already-suppressed person is
-- therefore a no-op that keeps the original row (`ON CONFLICT DO NOTHING`),
-- because the earliest reason is the moment the tenant lost the right to mail
-- them. A hard bounce arriving three months after somebody unsubscribed must
-- not rewrite the record into "their mailbox was full" — that reads as a
-- technical problem somebody might try to fix, and the person asked to be left
-- alone.
--
-- The address is stored ALREADY NORMALISED (lower(btrim(...)), the same fold
-- `campaign_audience` applies to its three sources and `campaign_consent` to
-- its records), and the CHECK holds it to that. A suppression row that does not
-- join is not a near miss: it is somebody who asked to stop and is still being
-- mailed.
--
-- There is deliberately no `recorded_by`. The write lives on `TenantStore`
-- rather than `AccountStore` because the loudest source of these rows has no
-- logged-in colleague behind it at all — the one-click unsubscribe endpoint
-- (RFC 8058, queue item C2s.2) works with no account and no login. A column
-- that would be NULL for the most important case is not provenance, it is a
-- column. Who acted is answered by `reason` and `source_ref`.
CREATE TABLE campaign_suppression (
    tenant_id  TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    -- An opaque id for the record, so a screen can say "excluded because of
    -- this" and link to it, exactly as a recipient carries its consent id.
    -- The address is the key; this is the handle.
    id         TEXT NOT NULL,
    -- The person, normalised exactly as every other campaign query normalises
    -- an address.
    address    TEXT NOT NULL,
    -- Why they may never be mailed again. The three ADR 0044 §2 names, plus
    -- `manual` for the person who phones and asks to be taken off the list:
    -- recording that as an `unsubscribe` would put it in the number a sending
    -- reputation is judged on, and a rate that counts phone calls as clicks is
    -- a lie told to ourselves.
    reason     TEXT NOT NULL,
    -- Which send, which bounce report, which conversation. NULL where there is
    -- honestly nothing to point at; the per-recipient send record (queue item
    -- C5m.1) is what will fill it in for bounces and complaints.
    source_ref TEXT,
    -- When it happened — when they clicked, when the mail bounced, when they
    -- phoned. Distinct from `recorded_at` because a bounce report is processed
    -- after the fact and dating suppression from the processing would misstate
    -- how long we have been mailing somebody who could not receive it.
    occurred_at TIMESTAMPTZ NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- The address is the primary key: one person, one answer, and an
    -- `ON CONFLICT` that cannot pick the wrong row.
    PRIMARY KEY (tenant_id, address),
    -- The handle is unique too, so the id in a screen's "excluded because of"
    -- link identifies exactly one record.
    CONSTRAINT campaign_suppression_id UNIQUE (tenant_id, id),
    CONSTRAINT campaign_suppression_reason CHECK (
        reason IN ('unsubscribe', 'hard_bounce', 'complaint', 'manual')
    ),
    CONSTRAINT campaign_suppression_address_normalised CHECK (
        address = lower(btrim(address)) AND address <> '' AND octet_length(address) <= 320
    )
);

-- The audience joins this table on every read, tenant first. The primary key
-- already indexes (tenant_id, address); this one answers the other question a
-- screen asks — "who has this tenant suppressed lately, and why" — without
-- walking the whole table.
CREATE INDEX campaign_suppression_by_time
    ON campaign_suppression (tenant_id, occurred_at DESC, address);
