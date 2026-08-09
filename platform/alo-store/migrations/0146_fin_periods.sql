-- alo Finance (ADR 0035, wave B4.10): the fiscal periods and the soft close
-- (docs/design/finance.md, "Fiscal periods and the soft close").
--
-- WHY A TABLE AND NOT A DATE IN SETTINGS. A single `lock_before` date answers
-- the posting question — may this entry be written? — and none of the others a
-- bookkeeper actually asks: which quarter is closed, who closed it, when, and
-- what did they say about it. Named periods are also what the four reports
-- (B4.11) offer as a picker. So the periods are rows, and the lock date is
-- DERIVED from them: `max(to_date)` over the closed ones. There is no lock-date
-- column, because a stored derivation is a second answer waiting to disagree
-- with the first.
--
-- THE CLOSE IS SOFT. `status` goes back to 'open' when an admin reopens the
-- period with a reason, and the reopen is audited (B2.13) like every other
-- mutating `/finance/*` act. A hard close was rejected in the design note: a
-- small business finds a missing receipt in week three of every quarter, and a
-- lock nobody can lift is a lock people work around by backdating into the next
-- period, which is worse than reopening this one.
--
-- WHO CLOSED IT IS STORED HERE, unlike the bank line's ignore reason (migration
-- 0145) where only the reason is a column. The difference is that "is Q2
-- closed, and by whom?" is part of the period's own state — a reader looking at
-- the period must see it without reading a log — whereas the audit trail
-- answers the history of how it got there. The two agree because the store
-- writes both from the same act.
--
-- ONE ROW'S NOTE IS THE NOTE OF ITS CURRENT STATE: the sentence the closer left,
-- or (after a reopen) the reason it was reopened. A period does not accumulate a
-- history of notes; that is what the audit log is.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE fin_periods (
    tenant_id  TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id         TEXT NOT NULL,
    -- Both ends inclusive: a quarter is 2026-04-01 through 2026-06-30, which is
    -- how an accountant states it and how the close rule reads it.
    from_date  DATE NOT NULL,
    to_date    DATE NOT NULL,
    status     TEXT NOT NULL DEFAULT 'open',
    -- Set together with the close, cleared together with the reopen.
    closed_by  TEXT,
    closed_at  TIMESTAMPTZ,
    -- The note of the current state (see the header).
    note       TEXT NOT NULL DEFAULT '',
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    CONSTRAINT fin_periods_status
        CHECK (status IN ('open', 'closed')),
    CONSTRAINT fin_periods_dates
        CHECK (from_date <= to_date),
    -- A period outside this range is a typo in a date field, not a fiscal year.
    CONSTRAINT fin_periods_dates_sane
        CHECK (from_date >= DATE '1900-01-01' AND to_date <= DATE '2200-12-31'),
    -- "Closed" and "closed by somebody at some moment" are one fact, so the
    -- reopen cannot leave half of it behind.
    CONSTRAINT fin_periods_closed_shape
        CHECK ((status = 'closed') = (closed_by IS NOT NULL AND closed_at IS NOT NULL)),
    CONSTRAINT fin_periods_note_shape
        CHECK (char_length(note) <= 200),
    -- Two periods starting on one day are the same period entered twice. The
    -- full non-overlap rule needs a range type and lives in `fin_periods.rs`,
    -- serialised on the tenant row; this is the part a plain unique can hold.
    UNIQUE (tenant_id, from_date)
);

-- "What are the books closed through?" — asked by EVERY posting (the whole
-- journal write path reads it), so it is an index seek and not a scan.
CREATE INDEX fin_periods_closed_through
    ON fin_periods (tenant_id, status, to_date DESC);
