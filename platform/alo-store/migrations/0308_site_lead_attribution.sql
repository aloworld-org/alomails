-- The seam between a website and the business it feeds (ADR 0036, S2.10b):
-- which contact-form submission became which CRM opportunity.
--
-- Sites owns this table; CRM and Billing own theirs and are not touched. The
-- link is the only new fact — everything else the funnel reports (the deal's
-- state and value, the customer's invoices) is read from the modules that own
-- it, through their own tenant-scoped tables, and is never copied here. A
-- second copy of a deal's value would be wrong the moment somebody edits the
-- deal.
--
-- Three decisions shape it:
--
--   * **One submission becomes at most one lead.** The unique constraint is
--     the whole rule: clicking twice must not raise a twin opportunity, and a
--     funnel that counts one enquiry as two leads is a lying funnel. Linking a
--     second, different deal to the same submission is refused in the store
--     with a message naming the rule.
--   * **The source is stored beside the submission**, denormalised, because it
--     is the key the aggregate counters are already written under
--     (`site_conversion_daily.source_kind` / `source_id`). Joining a link to a
--     count must not need the form row to still exist.
--   * **The link holds nothing about the visitor.** Name, address and message
--     stay in `site_form_submissions`, where a deletion request can reach
--     them; this row is two ids, a user and a time. Deleting the submission
--     takes the link with it (below), which is the correct reading of erasure:
--     the deal the tenant created remains theirs, but the claim "this person
--     wrote in" does not outlive the record of them writing in.
--
-- The aggregate conversion counters are unaffected by any of that — they were
-- never keyed by a submission and never held an identity (0307).
CREATE TABLE site_lead_attribution (
    tenant_id     TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id            TEXT NOT NULL,
    site_id       TEXT NOT NULL,
    -- The conversion point the enquiry came through, in the vocabulary
    -- 0307 counts under. A later commerce or booking source is an additive
    -- change to this check, exactly as it is there.
    source_kind   TEXT NOT NULL DEFAULT 'form' CHECK (source_kind IN ('form')),
    source_id     TEXT NOT NULL CHECK (length(source_id) BETWEEN 1 AND 64),
    submission_id TEXT NOT NULL,
    deal_id       TEXT NOT NULL,
    -- Who made the link. A handoff is a person's decision, not a machine's.
    linked_by     TEXT NOT NULL,
    linked_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- Every reference is composite, so a guessed id from another tenant cannot
    -- be linked even if a WHERE clause is wrong: the row simply cannot exist.
    FOREIGN KEY (tenant_id, site_id)
        REFERENCES sites (tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, submission_id)
        REFERENCES site_form_submissions (tenant_id, id) ON DELETE CASCADE,
    -- CRM owns the opportunity; deleting it removes the claim that a form
    -- produced it, and never the other way round.
    FOREIGN KEY (tenant_id, deal_id)
        REFERENCES crm_deals (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT site_lead_attribution_one_lead_per_submission
        UNIQUE (tenant_id, submission_id)
);

-- The funnel reads one site over a period, newest first.
CREATE INDEX site_lead_attribution_by_site
    ON site_lead_attribution (tenant_id, site_id, linked_at DESC);
-- "Is this opportunity one the website brought in?" — the read a deal drawer
-- makes, and the lookup the CASCADE above uses.
CREATE INDEX site_lead_attribution_by_deal
    ON site_lead_attribution (tenant_id, deal_id);
