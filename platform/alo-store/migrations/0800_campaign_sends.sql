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
-- (campaign, address)" — keyed on who was MAILED, not on who was enrolled.
--
-- The distinction is the whole design, and getting it wrong is worse than not
-- having the constraint at all. Three candidates:
--
--   (send, address)     — the primary key above. Stops one send from enrolling
--                         somebody twice, and happily lets a SECOND send of the
--                         same campaign mail them again. That is the
--                         press-send, spot-the-typo, stop, fix, send-again
--                         accident: everyone who got the broken copy also gets
--                         the fixed one.
--
--   (campaign, address) — over EVERY row, whatever became of it. Prevents the
--                         double mail, and creates a far worse failure:
--                         enrolment writes every recipient as `pending` within
--                         seconds, long before the dispatcher sends anything,
--                         so stopping a send that had not yet mailed a soul
--                         leaves a full set of rows behind and the campaign can
--                         never be sent to anybody, ever. The safety button
--                         becomes the thing that kills the campaign.
--
--   (campaign, address) — this one. The guarantee people actually want: a
--   WHERE state = 'sent'  campaign reaches a given person at most once, ever,
--                         while somebody who was enrolled and never mailed
--                         remains reachable by a later attempt.
--
-- So: stop a send halfway and the next one reaches exactly the people the first
-- did not — which is what an operator means by "fix it and send it again", and
-- what neither of the other two gives them.
--
-- The consequence that IS deliberate: mailing the same people the same campaign
-- a second time is impossible, and doing it on purpose means writing a second
-- campaign. That is the honest model for bulk mail; a "resend" that quietly
-- re-enrols is how somebody receives four copies from a system that believes it
-- is behaving correctly.
--
-- Postgres allows any number of rows outside a partial index's predicate, so
-- the pending and skipped rows of an abandoned send never collide with a later
-- attempt.
CREATE UNIQUE INDEX campaign_send_recipients_mailed_once_per_campaign
    ON campaign_send_recipients (tenant_id, campaign_id, address)
    WHERE state = 'sent';

-- The dispatcher's claim query: the pending rows of one send, oldest first.
CREATE INDEX campaign_send_recipients_pending
    ON campaign_send_recipients (tenant_id, send_id, enrolled_at)
    WHERE state = 'pending';
