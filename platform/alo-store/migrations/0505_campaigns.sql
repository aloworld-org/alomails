-- The campaign itself: what the mail says (alo Campaigns, ADR 0044; queue item
-- C3.1). Waves C1 and C2s built who may be mailed and how somebody leaves;
-- this is the first table that holds a message.
--
-- NOTHING HERE SENDS, and nothing here is a send. There is no status column, no
-- scheduled_at, no segment_id and no per-recipient anything: a campaign that
-- could be marked "sent" would be a lifecycle invented ahead of the sending
-- path, which ADR 0044 §1 blocks on a second egress IP that has to be bought.
-- The per-recipient send record is queue item C5m.1 and gets its own table; the
-- segment a campaign is aimed at is a link that belongs beside it, on the day a
-- send exists to use it. Both are additive columns/tables later, which is the
-- expand-only migration this repository requires.
--
-- WHY THE BODY IS JSONB AND THE REST IS COLUMNS. The opposite choice from
-- `campaign_segments` (0502), and for the opposite reason. A segment's
-- conditions are a small closed set of rules somebody's inbox depends on, so
-- they are CHECK-constrainable columns. A campaign body is an ordered list of
-- blocks in the alo Docs block model (`web/src/authoring/document.ts`, ADR
-- 0015) — the same model, because the composer is that editor rather than a
-- second one — and the database cannot usefully constrain a block list. The
-- shape is therefore held by `campaign_content.rs`, which validates it on every
-- write, and by the envelope's `schema_version`, which is what lets a v2 body
-- be recognised rather than half-read. The only rules the database keeps are
-- the ones it can keep truthfully: the envelope is an object, it declares a
-- version, and its `blocks` is an array.
--
-- WHY A TOPIC IS REQUIRED. C2s.2's landing page offers a recipient "fewer,
-- rather than only none" — this kind of mail, or all of it — and the "kind"
-- comes from the campaign. A campaign with no topic can only ever offer
-- all-or-nothing, and a recipient offered all-or-nothing presses the spam
-- button instead: the one signal that ends a sending reputation. So the topic
-- is NOT NULL and non-blank here rather than defaulted in a caller, and it is
-- stored AS THE SENDER WROTE IT (whitespace-collapsed, case kept) for the same
-- reason `campaign_unsubscribe_tokens.topic` is: a human reads the label on the
-- page. `campaign_topic_optouts` keeps the fold, because a query compares it —
-- one rule (`normalise_topic`), applied to both, exactly as the address fold is.
CREATE TABLE campaigns (
    tenant_id  TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id         TEXT NOT NULL,
    -- The subject line, as it would arrive in an inbox. Also what a colleague
    -- recognises the campaign by in a list: a separate internal name would be a
    -- second thing to keep in step with the first, and the queue item names
    -- subject, preheader and content — not a title.
    subject    TEXT NOT NULL,
    -- The preview text mail clients show beside the subject. NULL is "none" —
    -- a real state, in which a client falls back to the first line of the body.
    -- Blank is not a third state and is refused: a preheader of spaces is the
    -- classic way to hide the fallback while looking like a value.
    preheader  TEXT,
    -- Which kind of mail this is, as the sender wrote it (see above).
    topic      TEXT NOT NULL,
    -- The body: `{"schema_version": 1, "blocks": [ … ]}` in the Docs block
    -- model. An empty block list is a legitimate draft — somebody who has named
    -- the campaign and not yet written it.
    content    JSONB NOT NULL DEFAULT '{"schema_version": 1, "blocks": []}'::jsonb,
    -- The colleague who wrote it. Who to ask what it was for; never a claim
    -- that anybody agreed to receive it (that is `campaign_consent`).
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    CONSTRAINT campaigns_subject CHECK (
        btrim(subject) <> '' AND char_length(subject) <= 200
    ),
    -- NULL or a real preheader; never blank (see above).
    CONSTRAINT campaigns_preheader CHECK (
        preheader IS NULL OR (btrim(preheader) <> '' AND char_length(preheader) <= 200)
    ),
    -- Held to what `normalise_topic` can fold: non-blank, and inside the same
    -- 80 characters `campaign_topic_optouts` holds its folded form to. A label
    -- that could not be folded would be a campaign whose unsubscribe page could
    -- not name what it is offering to stop.
    CONSTRAINT campaigns_topic CHECK (
        btrim(topic) <> '' AND char_length(topic) <= 80
    ),
    -- The envelope, to the depth SQL can check it honestly. Everything else —
    -- the block vocabulary, the caps, the ids — is `campaign_content.rs`, which
    -- is the only writer. This constraint exists so that a row written by some
    -- future path that forgot the envelope is refused rather than read as an
    -- empty campaign.
    CONSTRAINT campaigns_content_envelope CHECK (
        jsonb_typeof(content) = 'object'
        AND jsonb_typeof(content -> 'blocks') = 'array'
        AND jsonb_typeof(content -> 'schema_version') = 'number'
    )
);

-- The campaign list is "what have we written, newest first", per tenant.
CREATE INDEX campaigns_by_tenant ON campaigns (tenant_id, created_at DESC, id);
