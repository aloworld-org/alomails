-- alo HR (ADR 0035, wave B6.03b): asking for time off, and the decision on it —
-- the record the absence layer and every balance are folded from
-- (docs/design/hr.md, "Leave" → "The request, and its state machine").
--
-- Five decisions are recorded here rather than left to whoever reads it next.
--
-- 1. **A request stores its DAYS, never its cost.** There is no
--    `cost_minutes` column: what a request consumes is folded at read time from
--    the working pattern of the employment in force on each of its days, the
--    public holidays of that employment's calendar, and the days approved leave
--    already covers (`hr_leave_math::request_cost`). A frozen figure would be
--    the `qty_on_hand` mistake (B5.01) one table further on: a corrected
--    working pattern, a holiday added to a calendar or an employment ended
--    early would each leave a stored cost that nothing can reconcile, and the
--    person it is wrong for is the one person guaranteed to check it by hand.
--
-- 2. **Whole days, Monday-first minutes.** A request names a first and a last
--    day, and each day inside costs what that person normally works then. Half
--    days are therefore not a flag here: somebody who works four hours on a
--    Friday takes half a day by taking that Friday, and a tenant who wants
--    genuine half-days gets them when the working pattern says so. It is the
--    rejection `docs/design/hr.md` records under "Minutes, and the working
--    pattern that makes a day mean something".
--
-- 3. **The state machine lives in the CHECKs as well as in the store**, because
--    a row that says `requested` while naming a decider, or `approved` while
--    naming nobody, is a row no balance can be explained from. `requested` has
--    no decision on it; `approved` and `rejected` both carry who decided and
--    when; `withdrawn` and `cancelled` carry who closed it (a cancellation
--    keeps the approval that preceded it, so the history reads as what
--    happened).
--
-- 4. **`taken` is not a status.** Approved leave whose days have passed is
--    taken, and that is a comparison against today rather than a column a
--    nightly job writes. One less state to get wrong, and no job to run.
--
-- 5. **Overlap is refused in the store, not in a unique index.** Postgres could
--    enforce it with an exclusion constraint over a `daterange`, and it was
--    rejected: the rule is not "no two ranges touch" but "no two ranges of the
--    same person that are still alive touch", the refusal has to name the
--    request that already covers those days, and a `409` naming the offending
--    request is the difference between a screen that explains itself and one
--    that says "constraint violated". The index below is what makes that check
--    a lookup rather than a scan.
--
-- NUMBERING: the business track mints migrations from 0200 upward; everything
-- below 0200 belongs to the sites track (docs/autonomy/STATE.md, 2026-08-11,
-- B6.03a). This is 0202; the next business migration is 0203.

CREATE TABLE hr_leave_requests (
    tenant_id       TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id              TEXT NOT NULL,
    -- Whose absence this is. The employee, not a user: somebody without a login
    -- takes leave too, and HR records it for them.
    employee_id     TEXT NOT NULL,
    -- Which of the tenant's policies it comes off. An archived policy cannot be
    -- chosen for a new request (the store checks), and stays referenceable so a
    -- historical absence is still explicable beside the rule that granted it.
    policy_id       TEXT NOT NULL,
    -- The first and last day of the absence, inclusive. Both are DATE, never a
    -- timestamp: "the 3rd of March" must not shift across midnight in a zone.
    from_day        DATE NOT NULL,
    to_day          DATE NOT NULL,
    -- 'requested' | 'approved' | 'rejected' | 'withdrawn' | 'cancelled'.
    status          TEXT NOT NULL DEFAULT 'requested',
    -- What the person wrote when they asked. Read by their manager, and never
    -- returned by the absence layer — a reason is not what a team needs to plan.
    note            TEXT NOT NULL DEFAULT '',
    -- The user who asked. Usually the employee themselves through their own
    -- login; HR when they record an absence for somebody who has none.
    requested_by    TEXT NOT NULL,
    -- The user who approved or rejected, and when. A policy that requires no
    -- approval (a sick policy a tenant records rather than decides) names the
    -- requester here, so the record does not pretend somebody decided.
    decided_by      TEXT,
    decided_at      TIMESTAMPTZ,
    decision_note   TEXT NOT NULL DEFAULT '',
    -- Who took the request back (`withdrawn`) or cancelled the approved leave
    -- (`cancelled`), and when.
    closed_by       TEXT,
    closed_at       TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- Both ends are tenant-qualified, so a row can only ever point at a person
    -- and a policy of its own tenant: cross-tenant reference is unrepresentable
    -- rather than merely refused (Law 1).
    CONSTRAINT hr_leave_requests_employee_fk FOREIGN KEY (tenant_id, employee_id)
        REFERENCES hr_employees (tenant_id, id) ON DELETE CASCADE,
    -- RESTRICT, not CASCADE: a policy is archived rather than deleted precisely
    -- so the absences folded from it stay explicable.
    CONSTRAINT hr_leave_requests_policy_fk FOREIGN KEY (tenant_id, policy_id)
        REFERENCES hr_leave_policies (tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT hr_leave_requests_status CHECK (status IN (
        'requested', 'approved', 'rejected', 'withdrawn', 'cancelled'
    )),
    CONSTRAINT hr_leave_requests_range CHECK (to_day >= from_day),
    -- A year of leave in one request is already absurd; more is a typo that
    -- would make the day fold walk a decade (`hr_leave_math::REQUEST_MAX_DAYS`).
    CONSTRAINT hr_leave_requests_length CHECK (to_day - from_day < 366),
    CONSTRAINT hr_leave_requests_note_length CHECK (
        length(note) <= 2000 AND length(decision_note) <= 2000
    ),
    -- Decision 3, in the schema: an undecided request names no decider…
    CONSTRAINT hr_leave_requests_undecided CHECK (
        status <> 'requested' OR (decided_by IS NULL AND decided_at IS NULL)
    ),
    -- …a decided one always does…
    CONSTRAINT hr_leave_requests_decided CHECK (
        status NOT IN ('approved', 'rejected')
        OR (decided_by IS NOT NULL AND decided_at IS NOT NULL)
    ),
    -- …and a closed one records who closed it.
    CONSTRAINT hr_leave_requests_closed CHECK (
        (status IN ('withdrawn', 'cancelled'))
        = (closed_by IS NOT NULL AND closed_at IS NOT NULL)
    )
);

-- "What has this person asked for?" — the read behind their own list, the
-- overlap check on a new request, and every balance fold.
CREATE INDEX hr_leave_requests_by_employee
    ON hr_leave_requests (tenant_id, employee_id, from_day DESC);

-- "Who is away between these two days?" — the absence layer, and the approvals
-- queue. Partial, because a rejected or withdrawn request is history and no
-- planning read ever wants it.
CREATE INDEX hr_leave_requests_live
    ON hr_leave_requests (tenant_id, status, from_day, to_day)
    WHERE status IN ('requested', 'approved');
