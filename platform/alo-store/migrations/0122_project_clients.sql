-- alo Projects (ADR 0035, wave B3): the client facts of a project — who it is
-- worked for, in what currency, at what rate, against what budget.
--
-- This is a SECOND LENS on rows that already exist, not a second project list.
-- A client project is a `task_projects` row (ADR 0021/0022 — the board the team
-- opens every morning) with one row here beside it; a project without a row
-- here is exactly what an internal project is, with no sentinel value to
-- misread (docs/design/projects.md, "One project list, extended"). `tasks.rs`
-- gains no column and no reason to change, and the join is a LEFT JOIN.
--
-- The primary key is the project itself: an engagement has one client, one
-- currency and one budget, so "which client facts apply here" has exactly one
-- answer and the question cannot be asked twice.
--
-- Client facts may only be attached to a `team` project. `task_projects.kind`
-- governs VISIBILITY — `personal` resolves only for its owner — and an
-- engagement whose hours are approved by somebody else and billed to a
-- customer is not private work. The store refuses it with a named rule;
-- nothing here can express it either, because a personal board has no reason
-- to carry a customer.
--
-- Money is integer cents and time is integer minutes, always (Law: no float
-- touches money). There is no DOUBLE PRECISION column in this table at all.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE project_clients (
    tenant_id      TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    -- The `task_projects` row these facts describe. Also the key: one
    -- engagement, one client.
    project_id     TEXT NOT NULL,
    -- Who the work is billed to. Required — a project with no customer is an
    -- internal project, which is expressed by having no row here at all.
    customer_id    TEXT NOT NULL,
    -- ISO 4217, uppercased in the store. Snapshotted from the customer when
    -- the facts are written and thereafter the engagement's own: a customer
    -- who later changes billing currency does not silently restate a running
    -- project's rate, exactly as a billing line snapshots its price instead of
    -- joining to the price list.
    currency       TEXT NOT NULL,
    -- The engagement's default hourly rate, in integer cents of `currency`, or
    -- NULL when nobody has priced it yet. A rate is COPIED onto each time
    -- entry as it is written (B3.03), so changing this never rewrites an hour
    -- that was already logged.
    rate_cents     BIGINT,
    -- The budget, in hours (as minutes) and/or in money. Either, both, or
    -- neither: both are ADVISORY. Logging an hour past the budget is a fact
    -- about the engagement, not an error, and nothing in the store refuses it
    -- (docs/design/projects.md, "Budgets and the profitability report").
    budget_minutes BIGINT,
    budget_cents   BIGINT,
    -- The day the engagement starts, or NULL when nobody has said. A DATE, not
    -- a timestamp: an engagement starts on a day in the tenant's world, not at
    -- an instant in UTC.
    starts_on      DATE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, project_id),
    -- Within the tenant, always. The board owns the engagement: delete the
    -- project and its client facts go with it, because facts about a project
    -- that no longer exists are not facts about anything.
    CONSTRAINT project_clients_project_fk FOREIGN KEY (tenant_id, project_id)
        REFERENCES task_projects (tenant_id, id) ON DELETE CASCADE,
    -- The same shape billing's own documents and CRM's deals use for the
    -- customer link (0102_billing_invoices.sql, 0113_crm_deals.sql). Customers
    -- are archived, never deleted, so in practice this cascades only with the
    -- tenant.
    CONSTRAINT project_clients_customer_fk FOREIGN KEY (tenant_id, customer_id)
        REFERENCES billing_customers (tenant_id, id) ON DELETE CASCADE,
    -- Defence in depth: the store validates every one of these before writing,
    -- so a violation here means a bug in our code, not bad user input. The
    -- bounds are the design note's, and each has a reason: a rate shares the
    -- billing line's ceiling so a rate that becomes a line can never overflow
    -- it; 10^7 minutes is ~19 person-years; 10^11 cents is a billion-euro
    -- budget, four orders below i64::MAX after the report's arithmetic.
    CONSTRAINT project_clients_currency_shape CHECK (currency ~ '^[A-Z]{3}$'),
    CONSTRAINT project_clients_rate_range
        CHECK (rate_cents IS NULL OR (rate_cents >= 0 AND rate_cents <= 1000000000)),
    CONSTRAINT project_clients_budget_minutes_range
        CHECK (budget_minutes IS NULL OR (budget_minutes >= 0 AND budget_minutes <= 10000000)),
    CONSTRAINT project_clients_budget_cents_range
        CHECK (budget_cents IS NULL OR (budget_cents >= 0 AND budget_cents <= 100000000000))
);

-- Every engagement worked for one customer — the read the unbilled view (B3.06)
-- and a customer's own drawer both make.
CREATE INDEX project_clients_by_customer
    ON project_clients (tenant_id, customer_id);
