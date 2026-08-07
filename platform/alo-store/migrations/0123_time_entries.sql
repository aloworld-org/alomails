-- alo Projects (ADR 0035, wave B3): the hours themselves — one row per
-- completed piece of work.
--
-- This is the table the whole module exists for. `project_clients` (0122) says
-- who a project is worked for; this says who worked, on what day, for how long,
-- and at what rate that time was priced. Everything downstream — the week grid,
-- the approval, the invoice draft, the profitability report — is a fold over
-- these rows (docs/design/projects.md).
--
-- THE HOURS OF A PERSON ARE PERSONAL DATA. A record of when an employee worked
-- and on what is personal data under the GDPR and a works-council question in
-- several member states, so `user_id` is not a convenience column: it is the
-- key the account door binds on every statement. A person's own hours are
-- reached through their own `AccountStore` and nowhere else; the cross-user
-- reads (the approvals inbox, the per-person breakdown) live on the tenant door
-- behind `require_admin` and arrive at B3.05. Notes never reach a log.
--
-- MINUTES ARE THE STORED TRUTH; hours exist only on a document. Never seconds
-- (a stopwatch that records to the second invites a UI that bills to the
-- second), never a decimal, and never a float — the conversion to a billing
-- line's milli-hour quantity happens once, in one pure function, at B3.06.
-- BIGINT rather than the design note's loose "INT" so that minutes, budget
-- minutes (0122) and the i64 arithmetic above them are one type with no cast
-- anywhere on the path from a logged hour to an invoice line.
--
-- WORK_DATE IS A DATE, supplied by the client in the user's own zone, and it is
-- what every period boundary uses — the week, the report, the unbilled cut-off.
-- `started_at` is kept beside it as provenance when a timer or a calendar event
-- produced the entry, and is used for nothing else: an entry stopped at 00:30
-- in Berlin belongs to the previous working day, and a timesheet whose week
-- boundary moves with the server's zone is one an employee will dispute.

CREATE TABLE time_entries (
    tenant_id   TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id          TEXT NOT NULL,
    -- Who worked. Bound from the account door on every statement, never taken
    -- from request input.
    user_id     TEXT NOT NULL,
    -- The `task_projects` row the work was done on — a team board, or the
    -- worker's own personal one.
    project_id  TEXT NOT NULL,
    -- The task inside that project, when the worker named one. Deliberately
    -- carries NO foreign key: deleting a task must not delete the hour that was
    -- worked on it, and the composite-key `ON DELETE SET NULL` that would
    -- express "forget the link, keep the hour" would null `tenant_id` too. A
    -- dangling id simply resolves to nothing, exactly as `tasks.source_id` does.
    task_id     TEXT,
    -- The day the person says they worked. See the header: a worked day is a
    -- calendar fact, not an instant.
    work_date   DATE NOT NULL,
    -- Provenance only: the instant a timer started, or a calendar event began.
    -- NULL for a manual entry, and never a period boundary.
    started_at  TIMESTAMPTZ,
    -- 1…1440. Zero minutes is not work, and 24 h is the most a day holds — a
    -- night shift over midnight is two entries, one per day, which is also how
    -- it must be billed.
    minutes     BIGINT NOT NULL,
    billable    BOOLEAN NOT NULL DEFAULT true,
    -- The rate this hour was priced at, SNAPSHOTTED when the entry was written
    -- (the caller's explicit rate, else the project's, else nothing), with the
    -- currency it is expressed in. A later change to the project's rate never
    -- rewrites an entry, for the reason a billing line snapshots its price
    -- instead of joining to the price list.
    --
    -- A BILLABLE ENTRY WITH NO RATE IS LEGAL: the person logging the hour is
    -- frequently not the person who prices it, and refusing the entry would
    -- lose the hour to protect the price. What is not legal is BILLING it — the
    -- handoff (B3.06) demands a rate rather than guessing one, and the report
    -- counts unrated hours as unrated rather than pricing them at zero.
    rate_cents  BIGINT,
    currency    TEXT,
    -- What the person did. Free text, bounded in the store, and personal data:
    -- it can name a client, a colleague or a case, so it never reaches a log.
    note        TEXT NOT NULL DEFAULT '',
    -- 'active' (real work) or 'proposed' (an agent's suggestion awaiting a
    -- human, ADR 0023). A proposed entry is excluded from every aggregate — the
    -- week total, the submit, the unbilled fold, the report — until it is
    -- accepted, at which point its rate is resolved and it becomes ordinary. A
    -- machine's guess about somebody's Tuesday is a suggestion, and a
    -- suggestion that is invisibly already in a total is not a suggestion.
    state       TEXT NOT NULL DEFAULT 'active',
    -- Where a drafted entry came from ('event' for a calendar-drafted one,
    -- B3.10). Same shape as `tasks.source_kind`/`source_id`.
    source_kind TEXT,
    source_id   TEXT,
    -- The document that carries this hour, set by the handoff (B3.06) and
    -- cleared when that document is deleted or voided. No foreign key on
    -- purpose (docs/design/projects.md): `ON DELETE CASCADE` would delete the
    -- hours when a draft invoice is discarded, and the composite `SET NULL`
    -- would null `tenant_id`. The release is an explicit statement inside the
    -- transaction that removes the document.
    invoice_id  TEXT,
    billed_at   TIMESTAMPTZ,
    created_by  TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- Within the tenant, always. The board owns the work done on it, the same
    -- shape `project_clients` uses.
    CONSTRAINT time_entries_project_fk FOREIGN KEY (tenant_id, project_id)
        REFERENCES task_projects (tenant_id, id) ON DELETE CASCADE,
    -- Defence in depth: the store validates every one of these before writing,
    -- so a violation here means a bug in our code, not bad user input.
    CONSTRAINT time_entries_minutes_range CHECK (minutes >= 1 AND minutes <= 1440),
    CONSTRAINT time_entries_rate_range
        CHECK (rate_cents IS NULL OR (rate_cents >= 0 AND rate_cents <= 1000000000)),
    CONSTRAINT time_entries_currency_shape
        CHECK (currency IS NULL OR currency ~ '^[A-Z]{3}$'),
    -- A rate and its currency are one snapshot: an amount without a currency is
    -- not an amount, and a currency without an amount describes nothing.
    CONSTRAINT time_entries_rate_currency_together
        CHECK (num_nonnulls(rate_cents, currency) <> 1),
    CONSTRAINT time_entries_state_known CHECK (state IN ('active', 'proposed')),
    -- Billed is likewise one fact: the document and the day it took the hour.
    CONSTRAINT time_entries_billed_together
        CHECK (num_nonnulls(invoice_id, billed_at) <> 1),
    -- A proposal is not work yet, so it cannot already be on a document.
    CONSTRAINT time_entries_proposals_are_unbilled
        CHECK (state = 'active' OR invoice_id IS NULL)
);

-- "My week", and the submit that follows it: one person's days in a range.
CREATE INDEX time_entries_by_user_date
    ON time_entries (tenant_id, user_id, work_date);
-- The project's own hours: the budget bar, the profitability report, and the
-- unbilled view's grouping.
CREATE INDEX time_entries_by_project_date
    ON time_entries (tenant_id, project_id, work_date);
-- "Which hours does this invoice carry" — asked when a document is deleted or
-- voided and its hours are released back to unbilled. Partial, because the
-- overwhelming majority of rows are not on any document.
CREATE INDEX time_entries_by_invoice
    ON time_entries (tenant_id, invoice_id) WHERE invoice_id IS NOT NULL;
