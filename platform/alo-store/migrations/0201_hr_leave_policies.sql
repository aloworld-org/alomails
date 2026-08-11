-- alo HR (ADR 0035, wave B6.03a): the leave a tenant grants — the policies a
-- balance is folded from (docs/design/hr.md, "Leave").
--
-- Four decisions are recorded here rather than left to whoever reads it next.
--
-- 1. **Minutes, never days.** Leave is stored, computed and carried in integer
--    minutes, because a day is not a fixed quantity: it is whatever that person
--    normally works on that weekday (`hr_employments.pattern_minutes`). A
--    four-hour Friday, a 30-hour contract and a mid-year move from five days to
--    four are ordinary in Europe, and all three turn a "days, with a half-day
--    flag" model into either a wrong balance or a manual correction. It is the
--    rule money has had since B1 — integer cents, never floats — applied to the
--    second quantity in this product that people check by hand.
--
-- 2. **The entitlement is a full-year figure at a full-time pattern**, not this
--    person's number. Somebody's entitlement is that figure scaled by their
--    working pattern and pro-rated by the days they were employed inside the
--    leave year, computed by `hr_leave_math.rs` where it can be property-tested
--    without a fixture. Storing a per-person figure would need rewriting every
--    time a pattern changed, and would make a balance computed last March
--    unexplainable next March.
--
-- 3. **No balance column, anywhere.** A balance is always recomputable from the
--    requests, the policies and the employments that were in force on each day.
--    A `balance_minutes` column decremented on approval is the `qty_on_hand`
--    mistake (B5.01) with somebody's holiday in it: one missed decrement on a
--    cancelled request and the number is wrong forever, with nothing to
--    reconcile it against.
--
-- 4. **The leave year starts on a day that exists in every year** (1..=28). A
--    1 January default, an April start and the other national starts all fit;
--    a 29 February start would be a date that three years in four cannot be
--    constructed from, and the balance fold would have to guess. The bound is
--    stated here and in `hr_leave_math::LeaveYear`.
--
-- ARCHIVED, RARELY DELETED. A balance is only explicable beside the policy that
-- produced it, so a policy an employment has ever been on is archived rather
-- than removed — `archived_at` carries WHEN, which is what a reader comparing a
-- historical balance needs.
--
-- NUMBERING: the business track mints migrations from **0200** upward;
-- everything below 0200 belongs to the sites track. The two tracks collided on
-- 0161, then on 0169, and then — while fixing 0169 — on 0170 and 0171 within the
-- same hour, because both were minting into one shared block. Separating the
-- ranges is the fix; renaming into the same range again is not
-- (docs/autonomy/STATE.md, 2026-08-11, B6.03a).

CREATE TABLE hr_leave_policies (
    tenant_id       TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id              TEXT NOT NULL,
    -- What the tenant calls it: "Vakantiedagen", "Congés payés", "Krankheit".
    -- The name is the tenant's vocabulary; `kind` is ours.
    name            TEXT NOT NULL,
    -- A closed vocabulary, because a word no code knows is a term nothing can
    -- compute with: `annual` accrues and carries over, `sick` is usually
    -- recorded rather than approved, `unpaid` grants nothing and may go
    -- negative, `other_paid` is everything else a tenant grants (a moving day,
    -- a wedding day, statutory family leave).
    kind            TEXT NOT NULL,
    -- Minutes granted per full leave year at a full-time pattern. 0 is
    -- meaningful: an unpaid policy grants nothing and is bounded by approval,
    -- not by a balance.
    entitlement_minutes BIGINT NOT NULL DEFAULT 0,
    -- `up_front` grants the whole entitlement on the leave year's first day;
    -- `monthly` grants a twelfth at each month start, remainder carried so the
    -- twelve grants sum exactly to the year (`hr_leave_math`).
    accrual         TEXT NOT NULL DEFAULT 'monthly',
    -- The leave year's first day, as a month and a day-of-month. 1 January is
    -- the default; April and other starts exist.
    leave_year_start_month SMALLINT NOT NULL DEFAULT 1,
    leave_year_start_day   SMALLINT NOT NULL DEFAULT 1,
    -- The most a person may carry into the next leave year. 0 = no carryover.
    carryover_cap_minutes  BIGINT NOT NULL DEFAULT 0,
    -- How long carried leave survives into the new year before it lapses. NULL
    -- = it does not lapse. Many member states cap this at 15 or 18 months.
    carryover_expires_after_months INTEGER,
    -- May an approval take a balance below zero? True for unpaid and for the
    -- tenants who lend next year's days; false is the safe default.
    allow_negative  BOOLEAN NOT NULL DEFAULT FALSE,
    -- A sick policy is often recorded, not approved: a request on such a policy
    -- is created already approved, with the requester named as the decider, so
    -- the record does not pretend somebody decided (docs/design/hr.md).
    requires_approval BOOLEAN NOT NULL DEFAULT TRUE,
    paid            BOOLEAN NOT NULL DEFAULT TRUE,
    -- NULL while the policy is one a tenant runs; set when it is retired and
    -- kept for the balances already folded from it.
    archived_at     TIMESTAMPTZ,
    created_by      TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- Defence in depth: the store validates before writing, so a violation
    -- here is a bug in our code rather than user input.
    CONSTRAINT hr_leave_policies_kind CHECK (kind IN (
        'annual', 'sick', 'unpaid', 'other_paid'
    )),
    CONSTRAINT hr_leave_policies_accrual CHECK (accrual IN ('up_front', 'monthly')),
    CONSTRAINT hr_leave_policies_name_present CHECK (length(name) > 0),
    -- A policy granting more than a year of minutes (366 × 1 440) is a typo,
    -- and it would inflate every balance folded from it.
    CONSTRAINT hr_leave_policies_entitlement_range
        CHECK (entitlement_minutes BETWEEN 0 AND 527040),
    CONSTRAINT hr_leave_policies_carryover_range
        CHECK (carryover_cap_minutes BETWEEN 0 AND 527040),
    CONSTRAINT hr_leave_policies_carryover_expiry
        CHECK (carryover_expires_after_months IS NULL
               OR carryover_expires_after_months BETWEEN 1 AND 24),
    -- A day-of-month that exists in every year, in every month (decision 4).
    CONSTRAINT hr_leave_policies_leave_year_start CHECK (
        leave_year_start_month BETWEEN 1 AND 12
        AND leave_year_start_day BETWEEN 1 AND 28
    )
);

-- One live policy per name: two policies both called "Vakantiedagen" are two
-- answers to "which balance is this?". Archived rows keep their name out of the
-- way, so retiring a policy and starting a fresh one with the same name works.
CREATE UNIQUE INDEX hr_leave_policies_name_unique
    ON hr_leave_policies (tenant_id, lower(name))
    WHERE archived_at IS NULL;

-- "Which policies does this tenant run?" — the read the request form, the
-- balance fold and the policy screen all make.
CREATE INDEX hr_leave_policies_live
    ON hr_leave_policies (tenant_id, kind, lower(name))
    WHERE archived_at IS NULL;
