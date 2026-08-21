-- When a tenant's sending identity started warming (alo Campaigns, ADR 0044;
-- queue item C2.3). Design: `docs/design/sending-reputation-warm-up.md`.
--
-- WHY A DATE AND NOT A CEILING. The obvious table stores "this tenant may send
-- N a day" and lets somebody raise it. That is the wrong shape twice over: the
-- number would be edited by whoever is impatient on the day a campaign is
-- ready, and it would carry no record of what it was derived from. A start date
-- is a fact about the past that nobody is tempted to adjust, and the ceiling is
-- computed from it — so raising the limit means back-dating the start of a
-- warm-up, which is a thing somebody has to lie about deliberately rather than
-- nudge.
--
-- WHY PER TENANT AT ALL, when the deployment has one sending identity today.
-- Because the reputation being warmed belongs to the identity a tenant sends
-- from, and ADR 0044 §1 gives each tenant its own subdomain. A single global
-- date would mean the second tenant to arrive inherits the first one's warmth,
-- which is exactly the assumption a receiver does not make.
--
-- Expand-only: one new table. No existing row can violate it.
CREATE TABLE campaign_warm_up (
    tenant_id  TEXT PRIMARY KEY REFERENCES tenants (id) ON DELETE CASCADE,
    -- The day the identity could first sign and send — not the day the
    -- campaigns work started. A receiver's memory begins at the first message
    -- it saw, so the schedule counts from there.
    started_on DATE NOT NULL,
    -- Who recorded it, and when. A warm-up start is the input to every ceiling
    -- this tenant is held to, so it is a thing an auditor asks about.
    recorded_by TEXT NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    -- A warm-up cannot start in the future. The ceiling on day zero is the
    -- tightest one there is, so a future date would silently mean "send
    -- nothing" — a limit nobody set and nobody could explain.
    CONSTRAINT campaign_warm_up_not_future CHECK (started_on <= (now() AT TIME ZONE 'UTC')::date)
);
