-- alo Projects (ADR 0035, wave B3.05): one person's week, once it has a status.
--
-- The hours (0123) say what was worked; this says whether the person has handed
-- that week in and what an approver decided about it. It is the table the
-- module's central rule lives in: an entry whose `work_date` falls in a
-- submitted or approved week refuses to move (docs/design/projects.md, "The
-- week: submit, approve, lock").
--
-- THE LOCK IS THIS ROW, NOT A FLAG ON THE ENTRY. A `locked` boolean on
-- `time_entries` would be two places to be right, and reopening a week would
-- have to rewrite every row it contains — a rewrite that is not atomic with the
-- reopen is a week half-unlocked. The week's status is the single fact; an
-- entry's editability is derived from it, in one function, at every write.
--
-- A WEEK WITH NO ROW IS OPEN. The overwhelming majority of weeks are never
-- submitted at all (the current one, every future one, and every past one in a
-- tenant that does not use approvals), and a row per person per week since the
-- beginning of the engagement would be a table of nothing happening. The row
-- appears when the person first submits; `open` is therefore both "no row" and
-- a stored status, and they mean the same thing — a week that was submitted and
-- withdrawn is open exactly as one that never was.
--
-- ADDRESSED BY ITS MONDAY on the personal door, by `id` on the admin one, and
-- the difference is not an inconsistency: a week the user has never submitted
-- has no row and therefore no id, so `POST /projects/weeks/2026-08-03/submit`
-- is the only shape that can create one. An approver, by contrast, is always
-- looking at a row that already exists, and naming a colleague's week by
-- (person, date) in a URL would put an employee's identity in an access log.
--
-- WHO DECIDED IS ON THE ROW, not only in the audit trail. "Who approved my week,
-- and when" is a question an employee is entitled to have answered from the
-- record they are looking at, without a second query against a log they cannot
-- read.

CREATE TABLE time_weeks (
    tenant_id     TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id            TEXT NOT NULL,
    -- Whose week. Bound from the account door when the person submits, and read
    -- (never taken from input) on the admin door when somebody decides.
    user_id       TEXT NOT NULL,
    -- The Monday itself, so no consumer has to recompute a week boundary and
    -- none of them can disagree about one. ISO 8601 week-numbering weeks,
    -- Monday-start; the store refuses any other weekday.
    week_start    DATE NOT NULL,
    -- 'open' (handed in nothing, or taken it back, or an approval was reopened),
    -- 'submitted' (awaiting a decision — LOCKED), 'approved' (decided yes —
    -- LOCKED), 'rejected' (decided no; unlocked, so the person can fix it and
    -- submit again).
    status        TEXT NOT NULL DEFAULT 'open',
    -- When the person handed this week in. Kept through the decision — "how
    -- long did my week wait" is a fair question and the inbox orders by it —
    -- and cleared only when the week goes back to `open`, because a week that is
    -- open is not awaiting anything.
    submitted_at  TIMESTAMPTZ,
    -- The admin who decided, the moment they did, and what they said about it.
    -- All three are cleared on reopen, for `submitted_at`'s reason: a decision
    -- that no longer stands must not still be displayed on the record. The
    -- history of decisions that were later undone lives in the append-only
    -- audit log (B2.13), which is what an append-only log is for.
    decided_by    TEXT,
    decided_at    TIMESTAMPTZ,
    -- Why it was rejected, in the approver's words. Personal-adjacent free text
    -- like a time note: bounded in the store, and never logged.
    decision_note TEXT NOT NULL DEFAULT '',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- One row per person per week, always. This is what makes the submit an
    -- upsert rather than a check-then-insert, so two simultaneous submits of
    -- the same week produce one row and one conflict rather than two rows.
    CONSTRAINT time_weeks_one_per_person_week UNIQUE (tenant_id, user_id, week_start),
    -- Defence in depth: the store validates each of these before writing, so a
    -- violation here means a bug in our code and not bad user input.
    CONSTRAINT time_weeks_status_known
        CHECK (status IN ('open', 'submitted', 'approved', 'rejected')),
    -- A Monday. `EXTRACT(ISODOW)` is 1 on Monday in every locale, unlike DOW's
    -- 0-is-Sunday, so the constraint says what it means.
    CONSTRAINT time_weeks_starts_on_monday
        CHECK (EXTRACT(ISODOW FROM week_start) = 1),
    -- A submitted week has a submission instant; a decided one keeps the instant
    -- it was handed in at; an open one has none, because nothing is pending.
    CONSTRAINT time_weeks_submitted_has_an_instant
        CHECK (status <> 'submitted' OR submitted_at IS NOT NULL),
    CONSTRAINT time_weeks_open_awaits_nothing
        CHECK (status <> 'open' OR submitted_at IS NULL),
    -- A decision is one fact too — who, and when. `decision_note` is not in it:
    -- an approval that says nothing is the ordinary case.
    CONSTRAINT time_weeks_decision_is_whole
        CHECK (num_nonnulls(decided_by, decided_at) <> 1),
    -- Decided means decided by somebody: 'approved'/'rejected' carry the pair,
    -- and 'open'/'submitted' carry neither.
    CONSTRAINT time_weeks_decided_iff_decision
        CHECK ((status IN ('approved', 'rejected')) = (decided_by IS NOT NULL))
);

-- The lock's read: "what is the status of this person's week", asked on every
-- single write of an hour. Covered by the unique constraint's index above, so
-- no second index is created for it.

-- The approvals inbox: this tenant's submitted weeks, oldest first.
CREATE INDEX time_weeks_pending
    ON time_weeks (tenant_id, submitted_at) WHERE status = 'submitted';
