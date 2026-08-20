-- The record of a send (alo Campaigns, ADR 0044; queue item C4.1, and the
-- control half of C4.4). Design: `docs/design/campaign-send-job.md`.
--
-- Migration 0505 said of the campaigns table: "NOTHING HERE SENDS, and nothing
-- here is a send... The per-recipient send record gets its own table; the
-- segment a campaign is aimed at is a link that belongs beside it, on the day a
-- send exists to use it. Both are additive columns/tables later." This is that
-- day, and this is that table. The campaign still has no status column: a
-- campaign is a letter, and a send is an act performed with it. One letter can
-- be the subject of an act that was stopped halfway and of nothing else ever
-- again, and collapsing the two would make "sent" a property of the prose.
--
-- WHAT THIS DOES NOT DO. It does not send. There is no dispatcher, no rendering
-- and no submission here or in the module above it — those are C4.2/C4.3 and
-- they consume this ledger. What is built is the ledger itself, because the
-- ledger is what makes a crash answerable by reading a table instead of by
-- guessing whether anybody was already mailed.
--
-- Expand-only: two new tables and nothing else. No existing row can violate
-- either, because no row has ever existed in them.

-- One act of sending one campaign.
CREATE TABLE campaign_sends (
    tenant_id    TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id           TEXT NOT NULL,
    campaign_id  TEXT NOT NULL,
    -- The topic as it was folded AT ENROLMENT, not read live from the campaign.
    --
    -- A topic can be edited on the campaign afterwards, and if this column
    -- followed it, the send's own record of which opt-outs it honoured would
    -- change retroactively — so a recipient skipped for declining "Newsletter"
    -- would later appear to have been skipped for declining something they
    -- never saw. What a send did is a fact about the past and is stored as one.
    topic_fold   TEXT NOT NULL,
    -- enrolling → sending → paused ⇄ sending → stopped | done
    --
    -- `enrolling` is its own state rather than a flag on `sending`, because the
    -- two fail differently and an operator must be able to tell them apart: a
    -- send stuck in `enrolling` is a caller that stopped walking pages, while
    -- one stuck in `sending` is a dispatcher that stopped. Naming them the same
    -- thing would hide one behind the other.
    state        TEXT NOT NULL,
    -- Why it stopped, when a person stopped it. NULL while it has not.
    stopped_note TEXT,
    opened_by    TEXT NOT NULL,
    opened_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Set when enrolment finished walking the audience; NULL until then. This
    -- is what separates "nobody is enrolled yet" from "nobody was eligible",
    -- which are the same row count and completely different facts.
    enrolled_at  TIMESTAMPTZ,

    PRIMARY KEY (tenant_id, id),

    -- Structural tenancy: the composite reference pins the campaign to the SAME
    -- tenant in the database, so even a bug in a WHERE clause cannot open a
    -- send against another tenant's letter.
    CONSTRAINT campaign_sends_campaign_fk
        FOREIGN KEY (tenant_id, campaign_id)
        REFERENCES campaigns (tenant_id, id) ON DELETE CASCADE,

    CONSTRAINT campaign_sends_state CHECK (
        state IN ('enrolling', 'sending', 'paused', 'stopped', 'done')
    ),
    CONSTRAINT campaign_sends_topic_fold CHECK (
        btrim(topic_fold) <> '' AND char_length(topic_fold) <= 80
    ),
    -- A note is absent or is a note; never blank. A blank string here would read
    -- as "stopped, and nobody said why" while actually meaning "somebody typed
    -- nothing", and those deserve different answers on a screen.
    CONSTRAINT campaign_sends_stopped_note CHECK (
        stopped_note IS NULL
        OR (btrim(stopped_note) <> '' AND char_length(stopped_note) <= 500)
    ),
    -- A send that has finished enrolling cannot claim to have done so before it
    -- began.
    CONSTRAINT campaign_sends_enrolled_after_opened CHECK (
        enrolled_at IS NULL OR enrolled_at >= opened_at
    )
);

-- At most ONE live send per campaign. `stopped` and `done` are terminal and
-- excluded, so a campaign that was stopped can be opened again — but a campaign
-- cannot have two acts running at once, which is the state in which two
-- dispatchers race for the same recipients.
--
-- Postgres allows many rows outside a partial index's predicate, so any number
-- of finished sends may sit behind the live one.
CREATE UNIQUE INDEX campaign_sends_one_live_per_campaign
    ON campaign_sends (tenant_id, campaign_id)
    WHERE state IN ('enrolling', 'sending', 'paused');

CREATE INDEX campaign_sends_by_campaign
    ON campaign_sends (tenant_id, campaign_id, opened_at DESC);

-- One person, once, per campaign.
CREATE TABLE campaign_send_recipients (
    tenant_id   TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    send_id     TEXT NOT NULL,
    -- Carried beside `send_id` and not merely reachable through it, because the
    -- uniqueness that matters is per CAMPAIGN rather than per send — see the
    -- index below. A join could answer the same question; a column lets the
    -- database enforce it.
    campaign_id TEXT NOT NULL,
    -- Normalised by `campaign_audience::normalise_address`, the same rule the
    -- audience and the suppression list fold by. One rule, applied everywhere,
    -- is what makes "already mailed" mean the same thing at both ends.
    address     TEXT NOT NULL,
    -- pending → sent | failed | skipped
    --
    -- `skipped` is written at enrolment (they declined the topic); the other two
    -- are written by the dispatcher that does not exist yet. The column is here
    -- now because adding the states later would mean a ledger whose early rows
    -- could not say what happened to them.
    state       TEXT NOT NULL,
    -- Why, for anything that is not `pending`. Held to a short reason code
    -- rather than prose so a tally can group by it; the sentence a person reads
    -- is the interface's, in their own language.
    reason      TEXT,
    enrolled_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- When the dispatcher last moved this row. NULL while pending.
    settled_at  TIMESTAMPTZ,

    PRIMARY KEY (tenant_id, send_id, address),

    CONSTRAINT campaign_send_recipients_send_fk
        FOREIGN KEY (tenant_id, send_id)
        REFERENCES campaign_sends (tenant_id, id) ON DELETE CASCADE,

    CONSTRAINT campaign_send_recipients_campaign_fk
        FOREIGN KEY (tenant_id, campaign_id)
        REFERENCES campaigns (tenant_id, id) ON DELETE CASCADE,

    CONSTRAINT campaign_send_recipients_state CHECK (
        state IN ('pending', 'sent', 'failed', 'skipped')
    ),
    CONSTRAINT campaign_send_recipients_reason CHECK (
        reason IS NULL OR (btrim(reason) <> '' AND char_length(reason) <= 60)
    ),
    -- A row that has settled says when; one that has not, does not.
    CONSTRAINT campaign_send_recipients_settled CHECK (
        (state = 'pending') = (settled_at IS NULL)
    ),
    CONSTRAINT campaign_send_recipients_address CHECK (
        btrim(address) <> '' AND char_length(address) <= 320
    )
);

-- THE IDEMPOTENCY C4.1 ASKS FOR: "nobody is mailed twice — idempotency on
-- (campaign, address)".
--
-- Per CAMPAIGN, not per send, and the difference is the whole point. A unique
-- key on (send, address) — which the primary key above already gives — stops
-- one send from enrolling somebody twice, and would happily let a SECOND send
-- of the same campaign mail them again. That is precisely the accident that
-- happens when somebody presses send, sees the typo, stops it, fixes it and
-- presses send again: the people who already received the broken copy get the
-- fixed one as well, and the people who did not, do not.
--
-- The consequence is deliberate: a campaign reaches a given person at most
-- once, ever. Mailing the same people again is a new campaign, which is the
-- honest model — the alternative is a "resend" that quietly re-enrols, and that
-- is how somebody receives four copies from a system that believes it is
-- behaving correctly.
CREATE UNIQUE INDEX campaign_send_recipients_once_per_campaign
    ON campaign_send_recipients (tenant_id, campaign_id, address);

-- The dispatcher's claim query: the pending rows of one send, oldest first.
CREATE INDEX campaign_send_recipients_pending
    ON campaign_send_recipients (tenant_id, send_id, enrolled_at)
    WHERE state = 'pending';
