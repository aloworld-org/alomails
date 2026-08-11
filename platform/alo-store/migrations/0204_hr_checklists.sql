-- alo HR (ADR 0035, wave B6.05): onboarding and offboarding checklists — the
-- shape of the first week and of the last one (docs/design/hr.md, "Onboarding
-- and offboarding checklists").
--
-- Five decisions are recorded here rather than left to whoever reads it next.
--
-- 1. **A template is a shape; an instance is a task project.** Instantiating a
--    template writes no rows in this file: it creates a real board in the Tasks
--    module, with the steps as tasks, assigned, dated, and linked back to the
--    person by the source link every task already carries (`source_kind =
--    'hr_employee'`, ADR 0021). The rejected alternative — `hr_checklist_items`
--    with its own status, assignee, due date and comments — is a fifth board in
--    a product that has one, and it would need its own notifications, its own
--    overdue view and its own mobile screen. A step that arrives as a task
--    arrives where its owner already looks.
--
-- 2. **Steps are rows, not a JSON column.** They are ordered, individually
--    validated, individually edited, and the instantiation reads them one at a
--    time to resolve an owner and compute a date. The typed-JSON shape used for
--    insight tiles and site sections earns its keep where the shape varies by
--    kind; here every step has the same four fields, and four columns say so
--    where a schema-in-a-string would only imply it.
--
-- 3. **The date is an offset, never a date.** A step is "the day before they
--    start" or "two days after their last day" — a fact about the shape that
--    stays true for every person it is ever run for. Negative offsets are
--    ordinary and are the point: ordering a laptop happens before the first
--    day, and a template that could only describe the days after it would push
--    every preparation step into the week it was meant to prepare.
--
-- 4. **The owner is a role, resolved at instantiation.** `manager` is whoever
--    that person reports to on the day the checklist is drawn; storing a user id
--    on the template would quietly assign three years of onboarding tasks to
--    somebody who left. `it` has no role in `tenant_user_roles` — it is resolved
--    from the caller's stated assignment, and falls back to the person drawing
--    the checklist, who is then looking at the one screen where a wrong
--    assignment is obvious.
--
-- 5. **No `archived_at` here**, unlike leave policies. A policy is archived
--    because a balance folded from it is only explicable beside it; a checklist
--    template explains nothing after the fact, because an instance is a *copy* —
--    deleting the template leaves every board it ever produced untouched. So the
--    verb is DELETE, and it is honest.
--
-- WHAT IS DELIBERATELY NOT HERE: account provisioning. "Create the mailbox",
-- "grant the Spaces" and "hand over the laptop" are steps a person does and
-- ticks. An HR write that provisions an account turns a badly-scoped HR role
-- into a security incident, and the module declines to have that capability at
-- all (docs/design/hr.md, "Cuts").
--
-- NUMBERING: the business track mints migrations from 0200 upward; everything
-- below 0200 belongs to the sites track.

CREATE TABLE hr_checklist_templates (
    tenant_id       TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id              TEXT NOT NULL,
    -- What the tenant calls it: "Nieuwe collega", "Arrivée", "Leaver — UK".
    name            TEXT NOT NULL,
    -- A closed vocabulary, because the word decides what the anchor date means:
    -- `onboarding` counts from the day they start, `offboarding` from their last
    -- day.
    kind            TEXT NOT NULL,
    created_by      TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- Defence in depth: the store validates before writing, so a violation here
    -- is a bug in our code rather than user input.
    CONSTRAINT hr_checklist_templates_kind CHECK (kind IN ('onboarding', 'offboarding')),
    CONSTRAINT hr_checklist_templates_name_present CHECK (length(name) > 0)
);

-- One template per name within a kind: two templates both called "Nieuwe
-- collega" are two answers to "which one do I run?". An onboarding and an
-- offboarding template may share a name, because the picker that offers them
-- has already chosen the kind.
CREATE UNIQUE INDEX hr_checklist_templates_name_unique
    ON hr_checklist_templates (tenant_id, kind, lower(name));

CREATE TABLE hr_checklist_steps (
    tenant_id       TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id              TEXT NOT NULL,
    template_id     TEXT NOT NULL,
    -- Order within the template. Whole numbers, rewritten as a block whenever
    -- the template is edited: a checklist is a short ordered list somebody reads
    -- top to bottom, not a board with fractional drag positions.
    position        INTEGER NOT NULL,
    -- What the step is, as it will read on the task card.
    title           TEXT NOT NULL,
    -- The longer form: what "prepare the workstation" means in this company.
    -- Becomes the task's description, so it arrives with the work.
    detail          TEXT NOT NULL DEFAULT '',
    -- Who does it, by role (decision 4). Resolved to a person at instantiation.
    owner_role      TEXT NOT NULL,
    -- Whole days from the anchor date; negative is before it (decision 3).
    day_offset      INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, template_id)
        REFERENCES hr_checklist_templates (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT hr_checklist_steps_owner CHECK (owner_role IN (
        'hr', 'manager', 'it', 'employee'
    )),
    CONSTRAINT hr_checklist_steps_title_present CHECK (length(title) > 0),
    -- A year either side of the anchor. A checklist step further from somebody's
    -- start date than that is a typo, and it would land a task in a week nobody
    -- will look at.
    CONSTRAINT hr_checklist_steps_offset_range CHECK (day_offset BETWEEN -365 AND 365),
    CONSTRAINT hr_checklist_steps_position_range CHECK (position >= 0)
);

-- "The steps of this template, in order" — the only read this table serves, made
-- by the template screen and by every instantiation.
CREATE INDEX hr_checklist_steps_by_template
    ON hr_checklist_steps (tenant_id, template_id, position);
