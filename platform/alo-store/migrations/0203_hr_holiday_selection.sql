-- alo HR (ADR 0035, wave B6.04): which public-holiday calendars a tenant
-- observes (docs/design/hr.md, "Public holidays").
--
-- Three decisions are recorded here rather than left to whoever reads it next.
--
-- 1. **The holidays themselves are not in the database.** They are a seed table
--    in the repo (`hr_holiday_seed.rs`) with each country's instrument named
--    beside it, and the movable feasts are computed from the Gregorian computus
--    rather than listed. A `hr_holidays` table with a row per country per year
--    would be a table that silently runs out — the year nobody loaded looks
--    exactly like a year with no holidays, and a balance computed from it is
--    quietly wrong. What is per-tenant is only the *choice*, which is what this
--    table holds.
--
-- 2. **One row per tenant, not one row per observed calendar.** The choice is a
--    single fact with two halves — the calendars this company observes, and the
--    one its leave arithmetic uses — and the halves must be written together or
--    a tenant can end up observing calendars with no default among them. A row
--    per calendar would also make "we deliberately observe none" (an empty
--    array here) indistinguishable from "nobody has chosen yet" (no row), and
--    those two must differ: the first is answered as it stands, the second is
--    seeded from the tenant's country on first read.
--
-- 3. **The default must be one of the observed calendars**, enforced here rather
--    than only in the store: a default nothing observes is a balance folded
--    against a calendar the company does not keep.
--
-- Codes are ISO 3166-1 alpha-2, uppercase, and are checked for *shape* here and
-- for *existence* in the store — a CHECK constraint listing fifteen country
-- codes would have to be migrated every time the seed grows a country.

CREATE TABLE hr_holiday_selection (
    tenant_id        TEXT PRIMARY KEY REFERENCES tenants (id) ON DELETE CASCADE,
    -- The calendars this tenant observes. Empty = observes none, deliberately:
    -- leave is then folded on the working pattern alone, which is correct
    -- rather than degraded.
    calendars        TEXT[] NOT NULL DEFAULT '{}',
    -- The one whose days the leave arithmetic uses. NULL only while nothing is
    -- observed.
    default_calendar TEXT,
    -- Who made the choice, and when it last changed.
    chosen_by        TEXT NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT hr_holiday_selection_codes CHECK (
        array_to_string(calendars, ',') ~ '^([A-Z]{2}(,[A-Z]{2})*)?$'
    ),
    CONSTRAINT hr_holiday_selection_max CHECK (cardinality(calendars) <= 10),
    CONSTRAINT hr_holiday_selection_default_observed CHECK (
        default_calendar IS NULL OR default_calendar = ANY (calendars)
    ),
    CONSTRAINT hr_holiday_selection_default_present CHECK (
        cardinality(calendars) = 0 OR default_calendar IS NOT NULL
    )
);
