-- A saved question about who to mail (alo Campaigns, ADR 0044; queue item
-- C1.4).
--
-- ADR 0044's central claim is that "there is nothing to sync, because there is
-- no list. A segment is a query over contacts alo already holds":
--
--     everyone who opened the last campaign, has not bought in ninety days,
--     and is in Belgium
--
-- This table stores that sentence, and stores it as CONDITIONS RATHER THAN AS
-- PEOPLE. There is no membership table here, and its absence is the whole
-- design: a stored member list is a copy of the audience that goes stale, and a
-- stale copy is how somebody who unsubscribed on Monday is mailed on Tuesday.
-- Every read of a segment re-asks the question of `campaign_audience`, so
-- consent (0500) and suppression (0501) are applied at the moment of asking and
-- cannot be outrun by a saved answer.
--
-- WHY TYPED COLUMNS RATHER THAN A JSON DEFINITION. The set of conditions ADR
-- 0044 names is small and closed, and each one is a rule somebody's inbox
-- depends on. Columns can be CHECK-constrained — a country that is not a
-- country, a period of minus ten days and a segment that says 'not_bought'
-- without saying since when are all refused by the database rather than by
-- whichever caller happens to be careful. A JSON blob would move all of that
-- into Rust and leave rows nobody can trust behind after the first schema
-- change. Adding a condition later (see below) is an additive column, which is
-- the expand-only migration this repository already requires.
--
-- WHAT IS DELIBERATELY MISSING: "has or has not received a given campaign".
-- ADR 0044 names it and it is genuinely part of the differentiator, but there
-- is no campaign to name yet — the campaign record is queue item C3.1 and the
-- per-recipient send record is C5m.1. A column referencing a table that does
-- not exist is not a schema, it is a guess, and the honest move is to add
-- `received_campaign_id` / `received` alongside these when there is something
-- for them to point at. The conditions below are exactly the ones today's data
-- can answer truthfully.
CREATE TABLE campaign_segments (
    tenant_id            TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id                   TEXT NOT NULL,
    -- What a colleague calls this question. Unique per tenant (folded, see the
    -- index below) so "send it to the Belgian customers" names one thing.
    name                 TEXT NOT NULL,
    -- ISO 3166-1 alpha-2, uppercase, as `billing_customers.country` stores it.
    -- EMPTY MEANS NO COUNTRY CONDITION, not "no country matches": an empty
    -- array is the absence of the question rather than a filter that excludes
    -- everybody. Only billing customers carry a country at all, so a country
    -- segment necessarily excludes people whose country is unknown — that is
    -- the honest reading (a person we cannot place is not evidence they are in
    -- Belgium) and `campaign_segments.rs` documents it where the query is
    -- built.
    countries            TEXT[] NOT NULL DEFAULT '{}',
    -- 'bought' or 'not_bought'; NULL is no purchase condition at all.
    purchase             TEXT,
    -- The period the purchase condition looks back over, in days. NULL with a
    -- purchase set means "ever" — "has never bought from us" is a real segment
    -- and refusing to express it would push callers into a period of 36 500
    -- days that means the same thing less clearly.
    purchase_within_days INTEGER,
    -- The colleague who saved it. Who to ask what the question was meant to
    -- mean; never a claim that the people it selects agreed to anything.
    created_by           TEXT NOT NULL,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    CONSTRAINT campaign_segments_name CHECK (
        btrim(name) <> '' AND char_length(name) <= 120
    ),
    CONSTRAINT campaign_segments_purchase CHECK (
        purchase IS NULL OR purchase IN ('bought', 'not_bought')
    ),
    -- A period with nothing to apply it to is a segment somebody misread when
    -- they saved it, and it would read on screen as if it filtered.
    CONSTRAINT campaign_segments_period_needs_a_purchase CHECK (
        purchase_within_days IS NULL OR purchase IS NOT NULL
    ),
    CONSTRAINT campaign_segments_period_range CHECK (
        purchase_within_days IS NULL
        OR (purchase_within_days >= 1 AND purchase_within_days <= 3650)
    ),
    -- The stored form is held to the shape the Rust validator produces: two
    -- uppercase letters each, so a lowercase 'be' saved by some future caller
    -- can never sit in a column that is compared with `=` against an uppercase
    -- one and quietly match nobody.
    CONSTRAINT campaign_segments_countries_shape CHECK (
        array_to_string(countries, ',') ~ '^([A-Z]{2}(,[A-Z]{2})*)?$'
        AND array_position(countries, NULL) IS NULL
        AND coalesce(array_length(countries, 1), 0) <= 50
    )
);

-- One segment per name, per tenant, folded and trimmed: two questions called
-- "Belgian customers" are a colleague about to send the wrong one.
CREATE UNIQUE INDEX campaign_segments_by_name
    ON campaign_segments (tenant_id, lower(btrim(name)));
