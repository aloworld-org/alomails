-- The assistant's lead capture joins the aggregate conversion record
-- (ADR 0040 §2, S3.03d): a 'chat' conversion source beside 0307's 'form'.
--
-- This is exactly the additive check-constraint change 0307 announced for
-- later conversion points. Rows written under 'form' keep meaning what they
-- meant; 'chat' rows count how often the conversation offered the lead form
-- (view) and how often a lead was actually raised (submit), keyed by the
-- site's own id — the one id a chat conversation is attributable to, since
-- the widget belongs to the site rather than to any section or form.
--
-- The privacy shape is unchanged and is the whole point of counting here
-- rather than anywhere new: no visitor identity, no time of day, no journey —
-- a 'chat' row can say "the assistant raised three leads on Tuesday" and can
-- never say who they were or what was asked (the lead itself lives in CRM,
-- where the tenant already keeps exactly the fields the visitor chose to
-- leave).
ALTER TABLE site_conversion_daily
    DROP CONSTRAINT site_conversion_daily_source_kind_check;
ALTER TABLE site_conversion_daily
    ADD CONSTRAINT site_conversion_daily_source_kind_check
    CHECK (source_kind IN ('form', 'chat'));
