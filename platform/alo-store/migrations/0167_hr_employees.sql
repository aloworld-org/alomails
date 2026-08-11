-- alo HR (ADR 0035, wave B6.02a): the people a tenant employs, and the terms
-- they are employed on (docs/design/hr.md, "The data model").
--
-- This is the most sensitive table in the suite. It holds home addresses,
-- dates of birth, national identifiers, bank accounts and pay. Four decisions
-- are recorded here rather than assumed by whoever reads it next.
--
-- 1. **Two tables, not one.** A promotion, a move to four days a week, a pay
--    rise and a fixed-term renewal are all changes to the *terms*, and a leave
--    balance computed last March must stay explicable next March — which needs
--    the working pattern that was in force THEN, not the one in force now. So
--    the person is one row that is edited, and the terms are rows that are
--    APPENDED: a change ends the current employment (`ended_on`) and starts the
--    next. It is the same shape B1 used for the FX snapshot and B3 for the rate
--    on a time entry: the figure that was true when the fact happened is stored
--    with the fact.
--
-- 2. **`user_id` is nullable.** A warehouse hand, a shop-floor worker or a
--    seasonal picker is employed, takes leave and appears on the payroll export
--    without ever opening a mailbox we host. Requiring a login would force a
--    tenant to buy a seat for somebody who cannot use one. It is UNIQUE per
--    tenant when set, so two employee records can never claim the same
--    colleague.
--
-- 3. **The org chart links employees, not users** (`manager_id` is an employee
--    id), so the chart is complete even where the accounts are not. The cycle
--    refusal is in the store, on write, and named in `hr_org.rs`: a chart that
--    can be cyclic is a chart whose renderer must defend itself forever.
--
-- 4. **No field here is a special category.** Nationality, ethnicity, religion,
--    union membership, health condition and disability status are absent by
--    decision (GDPR Art. 9), as are marital status and dependants (payroll's
--    business, and payroll calculation is a permanent non-goal). The list of
--    columns is closed; widening it is a design change, not a schema tweak
--    (docs/design/hr.md, "Data minimisation, and the fields we refuse").
--
-- ARCHIVED, NEVER DELETED. Employment records carry statutory retention in
-- every member state, commonly five to ten years after the contract ends.
-- Archiving is the only removal HR performs; an erasure when a retention period
-- has truly expired is an admin's deliberate act with legal advice, never a
-- scheduled job. `archived_at` carries WHEN — which is what a retention period
-- is counted from — where the design note's `status` word carried only THAT.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE hr_employees (
    tenant_id       TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id              TEXT NOT NULL,
    -- The colleague's login, when they have one. No composite FK is possible
    -- (`users` is keyed by a globally unique id alone), so the store proves the
    -- user is a member of THIS tenant before it writes — the rule
    -- `tenant_user_roles` established for the same reason.
    user_id         TEXT REFERENCES users (id) ON DELETE SET NULL,
    -- The tenant's own number for this person, as it appears on their payroll
    -- bureau's paperwork. Optional, unique per tenant when set.
    staff_number    TEXT,

    -- ---- public: the directory every member may read ----------------------
    given_name      TEXT NOT NULL,
    family_name     TEXT NOT NULL,
    -- What they are called, when that is not the given name. Blank means "use
    -- the given name" — a projection decision, not a stored duplicate.
    preferred_name  TEXT NOT NULL DEFAULT '',
    work_email      TEXT,
    work_phone      TEXT NOT NULL DEFAULT '',

    -- ---- private: the own door and the HR door, and nothing else ----------
    personal_email  TEXT,
    personal_phone  TEXT NOT NULL DEFAULT '',
    date_of_birth   DATE,
    address_line1   TEXT NOT NULL DEFAULT '',
    address_line2   TEXT NOT NULL DEFAULT '',
    postal_code     TEXT NOT NULL DEFAULT '',
    city            TEXT NOT NULL DEFAULT '',
    region          TEXT NOT NULL DEFAULT '',
    -- ISO 3166-1 alpha-2, uppercase, or blank. Not required: a person's home
    -- country is not a fact the employer always needs to record.
    country         TEXT NOT NULL DEFAULT '',
    -- The single most sensitive plain field in the schema. It exists because a
    -- payroll export without one is useless in most member states; it is HR-door
    -- only, never in a list response, and never in a log line.
    national_id     TEXT,
    -- Where wages are paid. Canonical (compact, uppercase), mod-97 checked by
    -- `iban.rs` before it is written.
    iban            TEXT,
    emergency_name  TEXT NOT NULL DEFAULT '',
    emergency_phone TEXT NOT NULL DEFAULT '',

    -- Who they report to. Self-referencing within the tenant; the cycle and
    -- depth rules are enforced by the store on write.
    manager_id      TEXT,
    -- An optional Drive node holding their photo. Optional by decision: a
    -- mandatory face on a directory is a discrimination surface, and whether to
    -- ask for one is the tenant's decision, not ours to require.
    photo_node_id   TEXT,

    -- NULL while employed here; set when the record leaves the directory, the
    -- org chart and the absence layer, and stays readable through the HR door.
    archived_at     TIMESTAMPTZ,
    created_by      TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- Tenant-first and composite: a manager link cannot cross a tenant boundary
    -- even if the store had a bug. ON DELETE RESTRICT is deliberate — a person
    -- is archived, not deleted, so a delete that would orphan reports is a
    -- database error rather than a silently flattened chart.
    CONSTRAINT hr_employees_manager_fk FOREIGN KEY (tenant_id, manager_id)
        REFERENCES hr_employees (tenant_id, id) ON DELETE RESTRICT,
    -- Nobody is their own manager. The store refuses the longer cycles too;
    -- this is the one the database can see by itself.
    CONSTRAINT hr_employees_manager_not_self CHECK (manager_id IS DISTINCT FROM id),
    -- Defence in depth: the store validates before writing, so a violation here
    -- is a bug in our code rather than user input.
    CONSTRAINT hr_employees_names_present
        CHECK (length(given_name) > 0 AND length(family_name) > 0),
    CONSTRAINT hr_employees_country_shape
        CHECK (country = '' OR country ~ '^[A-Z]{2}$')
);

-- One employee record per colleague: two records claiming the same login would
-- be two answers to "whose leave is this?".
CREATE UNIQUE INDEX hr_employees_user_unique
    ON hr_employees (tenant_id, user_id)
    WHERE user_id IS NOT NULL;

-- The tenant's own numbering, unique where it is used at all.
CREATE UNIQUE INDEX hr_employees_staff_number_unique
    ON hr_employees (tenant_id, staff_number)
    WHERE staff_number IS NOT NULL;

-- "Who is in the directory?" — the read every member gets, active only.
CREATE INDEX hr_employees_directory
    ON hr_employees (tenant_id, family_name, given_name)
    WHERE archived_at IS NULL;

-- "Who reports to this person?" — the org chart's fold and the manager door's
-- narrowing, which every leave decision passes through.
CREATE INDEX hr_employees_by_manager
    ON hr_employees (tenant_id, manager_id)
    WHERE manager_id IS NOT NULL;

-- "Which employee is this signed-in user?" — asked on the first request of
-- every session that opens /hr.
CREATE INDEX hr_employees_by_user
    ON hr_employees (tenant_id, user_id)
    WHERE user_id IS NOT NULL;


-- The terms, appended. What changes while the person does not.
CREATE TABLE hr_employments (
    tenant_id        TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id               TEXT NOT NULL,
    employee_id      TEXT NOT NULL,
    job_title        TEXT NOT NULL DEFAULT '',
    team             TEXT NOT NULL DEFAULT '',
    contract_kind    TEXT NOT NULL,
    started_on       DATE NOT NULL,
    -- NULL means "still running". A row is ended when the next one starts, so
    -- the periods of one employee never overlap (asserted by the store).
    ended_on         DATE,
    -- Minutes normally worked, Monday..Sunday. Seven entries, each 0..=1440.
    -- This is what makes "a day off" mean a number of minutes rather than a
    -- guess, and why a part-time change must start a new row instead of editing
    -- this one: a balance computed last March is folded from the pattern that
    -- was in force then.
    pattern_minutes  INTEGER[] NOT NULL,
    -- Private, HR door only. Integer cents like every other money figure in the
    -- suite (never a float, anywhere).
    pay_amount_cents BIGINT,
    pay_period       TEXT NOT NULL DEFAULT 'month',
    pay_currency     TEXT NOT NULL DEFAULT 'EUR',
    created_by       TEXT NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- CASCADE: terms mean nothing without the person, and the person is never
    -- deleted in ordinary operation (see the archive rule above).
    CONSTRAINT hr_employments_employee_fk FOREIGN KEY (tenant_id, employee_id)
        REFERENCES hr_employees (tenant_id, id) ON DELETE CASCADE,
    -- The vocabulary is closed for the same reason the role CHECK is: a word no
    -- code knows is a term nothing can compute with.
    CONSTRAINT hr_employments_contract_kind CHECK (contract_kind IN (
        'permanent', 'fixed_term', 'part_time', 'apprentice', 'contractor', 'intern'
    )),
    CONSTRAINT hr_employments_pay_period CHECK (pay_period IN ('hour', 'month', 'year')),
    CONSTRAINT hr_employments_pay_currency CHECK (pay_currency ~ '^[A-Z]{3}$'),
    CONSTRAINT hr_employments_dates CHECK (ended_on IS NULL OR ended_on >= started_on),
    CONSTRAINT hr_employments_pay_nonneg
        CHECK (pay_amount_cents IS NULL OR pay_amount_cents >= 0),
    -- Seven days, each a plausible number of minutes. A week is not a place to
    -- store 10 000 minutes of Monday.
    -- No subquery: a CHECK may not contain one, so the bounds are stated with
    -- scalar-array comparisons and the NULL slot is refused by name.
    CONSTRAINT hr_employments_pattern_shape CHECK (
        array_ndims(pattern_minutes) = 1
        AND array_length(pattern_minutes, 1) = 7
        AND array_position(pattern_minutes, NULL) IS NULL
        AND 0 <= ALL (pattern_minutes)
        AND 1440 >= ALL (pattern_minutes)
    )
);

-- "What were this person's terms, and on what date?" — the read every balance
-- fold and every payroll period makes, newest first.
CREATE INDEX hr_employments_by_employee
    ON hr_employments (tenant_id, employee_id, started_on DESC);

-- The current terms: at most one open row per employee, enforced rather than
-- assumed. Two open employments would be two working patterns on the same day,
-- and the balance fold would have to pick one.
CREATE UNIQUE INDEX hr_employments_one_open
    ON hr_employments (tenant_id, employee_id)
    WHERE ended_on IS NULL;
