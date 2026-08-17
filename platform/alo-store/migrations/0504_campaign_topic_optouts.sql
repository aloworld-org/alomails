-- Fewer, rather than only none (alo Campaigns, ADR 0044 §3; queue item C2s.2).
--
-- ADR 0044 §3 says the unsubscribe must work without a login and must not be a
-- maze. The queue item says the harder half: "offering FEWER rather than only
-- NONE — this kind of mail, or all of it. A recipient offered only
-- all-or-nothing presses the spam button instead, and that is the signal that
-- ends a sending reputation."
--
-- All-or-nothing is not a smaller feature, it is a worse one. Somebody who
-- wants the invoices but not the newsletter, and is offered only "stop
-- everything", has two options: keep receiving what they do not want, or take
-- the option that costs the sender its delivery — and the spam button is
-- one press while a reply asking to be taken off one list is a paragraph
-- nobody writes. So the landing page needs a narrower answer to offer, and a
-- narrower answer needs somewhere to live. This migration is that place.
--
-- TWO CHANGES, AND THE REASON THEY ARE DIFFERENT SHAPES.
--
-- 1. `campaign_unsubscribe_tokens.topic` — a COLUMN, added to the table 0503
--    created. The kind of mail is a property of the SEND: it is decided when
--    the message is built and it is the same for every recipient of it. The
--    token row is the only thing that knows which send a link came from
--    (`send_ref`), so putting the topic anywhere else would mean a second
--    lookup that could disagree with the first — and a landing page that named
--    the wrong kind of mail would take away a subscription the person meant to
--    keep. Nullable, because a send that does not name a kind is honest and the
--    page then offers only "all of it" rather than inventing a category to
--    narrow to.
--
-- 2. `campaign_topic_optouts` — a TABLE, not a column on anything. The opt-out
--    is a fact about a PERSON, and it has to outlive every token, every send
--    and every campaign: somebody who declines the newsletter in 2026 must
--    still be declining it in 2029, when the link they used is a row nobody
--    reads and the campaign it came from has been deleted. One person can also
--    decline several kinds, which a column cannot hold, and the whole point of
--    "fewer" is that they may decline one and keep the rest.
--
-- THE TOPIC IS FOLDED HERE AND NOT ON THE TOKEN, DELIBERATELY. The token keeps
-- the label as the sender wrote it, because it is shown to a human. The opt-out
-- keeps `lower(btrim(...))`, because it is compared. The failure that fold
-- prevents is exactly the one the address fold prevents in `campaign_audience`:
-- a person who declined "Newsletter" and is then sent "newsletter" has
-- unsubscribed from one copy of themselves, which is the thing ADR 0044's "there
-- is no list" claim exists to make impossible.
--
-- THE FIRST DECISION STANDS, AND THERE IS NO WAY BACK OUT. Same discipline as
-- `campaign_suppression` (0501): the write is `ON CONFLICT DO NOTHING` and there
-- is no update path and no delete path above this file. A person's decision
-- about what they want is not a row a bulk importer gets to tidy up, and
-- somebody who changes their mind says so through a form like anyone else —
-- which is evidence, where a tenant deleting the row is not.
--
-- WHAT IS DELIBERATELY MISSING: any enforcement. Nothing in this release reads
-- these rows to decide who gets a message, because nothing yet builds a message
-- that names a kind — the campaign record is queue item C3.1 and the send record
-- is C5m.1. The exclusion lands in `campaign_audience`'s `Reach` predicate, next
-- to consent and suppression, on the day a send can say which topic it is; doing
-- it now would mean a parameter threaded through four queries for a caller that
-- does not exist, which is the guess this queue refused when it left
-- `received_campaign_id` out of 0502. Nothing here sends anything either.

-- The kind of mail this link came from, as the sender wrote it. Shown to the
-- recipient, so it is not folded; bounded because it goes on a page and into a
-- sentence, not into a report.
ALTER TABLE campaign_unsubscribe_tokens
    ADD COLUMN topic TEXT;

ALTER TABLE campaign_unsubscribe_tokens
    ADD CONSTRAINT campaign_unsubscribe_tokens_topic CHECK (
        topic IS NULL OR (btrim(topic) <> '' AND char_length(topic) <= 80)
    );

CREATE TABLE campaign_topic_optouts (
    tenant_id   TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    -- The person, normalised exactly as every other campaign query normalises
    -- an address, so this row joins the audience rather than sitting beside it.
    address     TEXT NOT NULL,
    -- The kind of mail, FOLDED (see above). One person plus one topic is one
    -- decision, which is why the two of them plus the tenant are the key.
    topic       TEXT NOT NULL,
    -- The record's handle — safe to log, and what a screen links to when it
    -- names somebody as having declined a kind of mail.
    id          TEXT NOT NULL,
    -- Which link they used, as the unsubscribe token's RECORD id (never the
    -- token): "which send did they leave over" stays answerable without the
    -- working credential being copied into a second table. `NULL` where a
    -- colleague recorded the decision by some other route.
    source_ref  TEXT,
    -- When they decided, and when we were told. Two columns for the reason
    -- `campaign_consent` (0500) has two: a decision relayed by a colleague
    -- happened before the typing, and dating it from the typing overstates how
    -- fresh it is.
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, address, topic),
    CONSTRAINT campaign_topic_optouts_id UNIQUE (tenant_id, id),
    CONSTRAINT campaign_topic_optouts_address_normalised CHECK (
        address = lower(btrim(address)) AND address <> '' AND octet_length(address) <= 320
    ),
    -- The fold is held by the schema rather than trusted to the caller: a row
    -- whose topic is `Newsletter` would be a decision nothing matches, which is
    -- somebody who pressed the button and is still being mailed.
    CONSTRAINT campaign_topic_optouts_topic_folded CHECK (
        topic = lower(btrim(topic)) AND topic <> '' AND char_length(topic) <= 80
    ),
    CONSTRAINT campaign_topic_optouts_source_ref CHECK (
        source_ref IS NULL OR (btrim(source_ref) <> '' AND char_length(source_ref) <= 200)
    )
);

-- "What has this person declined" is the only question asked of this table, and
-- the primary key already answers it: `(tenant_id, address, …)` is a prefix
-- match. No index by topic, deliberately — a query that could list everybody who
-- declined a kind of mail is a list of people, and this queue's whole argument
-- is that there is no list.
