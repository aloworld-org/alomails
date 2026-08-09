-- alo Finance (ADR 0035, wave B4.07): a journey, and the rate that says what it
-- is worth (docs/design/finance.md, "Expenses, receipts and mileage").
--
-- MILEAGE IS A CLAIM AT A RATE TABLE, NOT AN EXPENSE WITH A MADE-UP AMOUNT.
-- Nobody paid €37.50 for driving 125 km; they drove 125 km, and a rate the
-- company published turns that into €37.50. So the two facts are stored apart:
-- `fin_mileage` holds the journey (the thing the traveller knows), and the money
-- lives on an ordinary `fin_expenses` row (0134) that the journey points at. The
-- claim then walks the same submit → approve → reimburse flow as a train ticket,
-- because from the moment the amount exists it IS one.
--
-- `fin_mileage_rates` is tenant-wide configuration, effective-dated: a per-km
-- rate is a number a member state changes on a New Year's Day, and a table with
-- one row per period is what lets a journey in December book at last year's rate
-- while January's books at this year's. THE TABLE SHIPS EMPTY. We seed no rate:
-- whether €0.30/km is the tax-free ceiling in a given member state is a
-- statement about that state's law on that date, and it is the tenant's
-- accountant who makes it, not us (the same rule `fin_categories` follows for
-- the words on the claim form).
--
-- THE RATE IS SNAPSHOTTED ONTO THE JOURNEY. `rate_cents_per_km` is a copy, not a
-- reference: correcting the table next spring must not silently restate what a
-- claim approved and paid out last autumn. It is the rule `billing_fx_rates`
-- follows for an issued invoice's exchange rate, for the same reason — a figure
-- somebody has already been paid is history.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

CREATE TABLE fin_mileage_rates (
    tenant_id      TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id             TEXT NOT NULL,
    -- The first day this rate applies. A journey takes the rate of the latest
    -- row whose `effective_from` is on or before the day it was driven; a
    -- journey before the earliest row has no rate and is refused rather than
    -- guessed at (a claim paid at a rate nobody published is money out of the
    -- door on our authority).
    effective_from DATE NOT NULL,
    -- What one kilometre is worth, in integer cents of the tenant's accounting
    -- currency. Never a float: this number is multiplied by a distance and paid
    -- to a person.
    cents_per_km   BIGINT NOT NULL,
    -- Why this rate, in the tenant's own words — "BMF-Schreiben 2026", "board
    -- decision of 12 Jan". Optional, and the reason the design note calls this a
    -- table shipped "empty with a note" rather than pre-filled.
    note           TEXT NOT NULL DEFAULT '',
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- Defence in depth: the store validates each of these before writing, so a
    -- violation here means a bug in our code rather than bad user input.
    --
    -- At least a cent. A rate of zero is not a cheaper allowance, it is the
    -- absence of one, and a claim worth nothing is not a claim (0134's
    -- `fin_expenses_gross_range`). The ceiling is the typo guard every alo money
    -- field carries, scaled to a per-kilometre figure: €100/km is already absurd.
    CONSTRAINT fin_mileage_rates_range
        CHECK (cents_per_km >= 1 AND cents_per_km <= 10000),
    CONSTRAINT fin_mileage_rates_note_shape CHECK (char_length(note) <= 200)
);

-- Two rates from the same day are a coin toss over what a person is paid.
CREATE UNIQUE INDEX fin_mileage_rates_day_unique
    ON fin_mileage_rates (tenant_id, effective_from);

CREATE TABLE fin_mileage (
    tenant_id         TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id                TEXT NOT NULL,
    -- Whose journey. Bound from the account door on every statement, never taken
    -- from request input — a journey names where somebody was on a date, which
    -- is personal data about them exactly as a receipt is (0134's header).
    user_id           TEXT NOT NULL,
    -- The day it was driven, in the traveller's own zone. It is what picks the
    -- rate, and it is the expense's `spent_on`.
    travelled_on      DATE NOT NULL,
    -- The distance, in thousandths of a kilometre — the `qty_milli` convention
    -- every other alo quantity uses, so 12.5 km is 12500 and no float is
    -- involved in what somebody is paid.
    km_milli          BIGINT NOT NULL,
    -- The rate in force on `travelled_on`, copied at the moment the claim was
    -- made (see the header). The amount on the expense is
    -- `km_milli * rate_cents_per_km / 1000`, rounded half-up.
    rate_cents_per_km BIGINT NOT NULL,
    -- Where from, where to, and what for. All three are personal data — they
    -- place a named person at an address on a date — and never reach a log.
    from_place        TEXT NOT NULL DEFAULT '',
    to_place          TEXT NOT NULL DEFAULT '',
    reason            TEXT NOT NULL DEFAULT '',
    -- The claim this journey became. NOT NULL and cascading: a journey with no
    -- claim is a fact nobody can act on, and deleting the claim deletes the
    -- journey that explains it rather than leaving a row that points at nothing.
    -- Composite, so the claim is always the same tenant's.
    expense_id        TEXT NOT NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    CONSTRAINT fin_mileage_expense_fk FOREIGN KEY (tenant_id, expense_id)
        REFERENCES fin_expenses (tenant_id, id) ON DELETE CASCADE,
    -- A journey of no distance is a typo, and the ceiling is the same typo guard
    -- the amount carries: 100 000 km is more than twice around the planet.
    CONSTRAINT fin_mileage_km_range
        CHECK (km_milli >= 1 AND km_milli <= 100000000),
    CONSTRAINT fin_mileage_rate_range
        CHECK (rate_cents_per_km >= 1 AND rate_cents_per_km <= 10000),
    CONSTRAINT fin_mileage_place_shape
        CHECK (char_length(from_place) <= 120 AND char_length(to_place) <= 120),
    CONSTRAINT fin_mileage_reason_shape CHECK (char_length(reason) <= 500)
);

-- One journey per claim, and one claim per journey: without this, two journeys
-- could explain the same amount and the second would be invisible money.
CREATE UNIQUE INDEX fin_mileage_expense_unique
    ON fin_mileage (tenant_id, expense_id);

-- "My journeys", newest first — the personal door's list, and the only read
-- this table has.
CREATE INDEX fin_mileage_by_user_date
    ON fin_mileage (tenant_id, user_id, travelled_on DESC);
