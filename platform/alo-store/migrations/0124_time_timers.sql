-- alo Projects (ADR 0035, wave B3.04): the running timer.
--
-- ONE ROW PER PERSON, OR NONE. A running timer is deliberately NOT a
-- `time_entries` row with a null duration (docs/design/projects.md, "The
-- running timer is not an entry"): that shape would make every aggregate in the
-- module — the week total, the budget bar, the profitability report, the
-- unbilled fold — responsible for remembering to exclude the row that is still
-- running, and the one that forgets bills a timer nobody has stopped. Here the
-- rule "one running timer per user" is the PRIMARY KEY: a second concurrent
-- start cannot represent itself, so the 409 the edge returns is a statement
-- about a race the database has already settled, not a check-then-write.
--
-- STOPPING IS WHAT WRITES THE HOUR, and it does so in one transaction with the
-- delete of this row: an hour that exists without its timer having been cleared
-- would be logged twice, and a timer cleared without its hour written is work
-- silently thrown away.
--
-- THE NOTE IS PERSONAL DATA, exactly as `time_entries.note` is — it can name a
-- client, a colleague or a case — so it never reaches a log; the spans on this
-- path carry ids and minute counts and nothing a human typed.

CREATE TABLE time_timers (
    tenant_id  TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    -- Whose timer. Bound from the account door on every statement, never taken
    -- from request input — a person's hours are their own
    -- (docs/design/projects.md, "Two doors, deliberately").
    user_id    TEXT NOT NULL,
    -- The board the clock is running against. CASCADE because a timer on a
    -- deleted project has nowhere to land its hour.
    project_id TEXT NOT NULL,
    -- The task inside that project, when the worker named one. No foreign key,
    -- for `time_entries.task_id`'s reason: deleting a task must not delete the
    -- record of work, and a dangling id simply resolves to nothing.
    task_id    TEXT,
    -- When the clock started. The entry a stop writes keeps this as
    -- provenance — never as the day the work belongs to, which the client
    -- states in the worker's own zone.
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Carried from the start so the entry a stop writes is complete without a
    -- second dialog.
    billable   BOOLEAN NOT NULL DEFAULT true,
    note       TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (tenant_id, user_id),
    CONSTRAINT time_timers_project_fk FOREIGN KEY (tenant_id, project_id)
        REFERENCES task_projects (tenant_id, id) ON DELETE CASCADE
);
