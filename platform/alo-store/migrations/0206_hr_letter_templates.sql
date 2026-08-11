-- alo HR (ADR 0035, wave B6.09b): the letters a tenant writes about its own
-- people — an employment confirmation, a reference, a letter for a landlord
-- (docs/design/hr.md, "The two tools that do ship").
--
-- Four decisions are recorded here rather than left to whoever reads it next.
--
-- 1. **The tenant writes the letter; the agent only fills it in.** A template is
--    a subject and a body typed by a person in this company, in this company's
--    language, saying what this company is willing to state about somebody.
--    `draft_letter_from_template` merges facts into it and leaves the result in
--    the caller's Drafts. There is no free-form generation path anywhere: a
--    template the tenant has not written is a `422`, never an improvisation, and
--    that is only true because the text a letter is made of lives in this table
--    rather than in a model's head.
--
-- 2. **The merge vocabulary is closed, and it is the directory.** Every
--    placeholder a body may carry resolves to a field the member directory
--    already shows everybody — name, work address, job title, team, start date —
--    plus the company's own letterhead facts and today's date. The rejected
--    alternative, "any employee column", is how a pay figure, a home address or
--    a national id ends up in a letter somebody drafted in a hurry; the design
--    note forbids pay outright, and a closed list is the only form of that rule
--    a later hand cannot widen by accident (`alo_store::hr_letters`).
--
-- 3. **The body is one column, validated on write.** Placeholders are parsed
--    when the template is saved, so a template that names a field this build
--    does not know is refused by the editor with the vocabulary in the message —
--    rather than at the moment somebody needed the letter. The stored text is
--    therefore always mergeable.
--
-- 4. **No `archived_at`.** Like a checklist template and unlike a leave policy:
--    a letter already drafted is a message in somebody's Drafts, a copy that
--    owes nothing to this row. So the verb is DELETE, and it is honest.
--
-- WHAT IS DELIBERATELY NOT HERE: pay. Not a column, not a placeholder, not a
-- join. A certificate that must state a salary is a letter a person completes,
-- not one the agent fills (`docs/design/hr.md`, "One tension in this section").
--
-- NUMBERING: the business track mints migrations from 0200 upward; everything
-- below 0200 belongs to the sites track.

CREATE TABLE hr_letter_templates (
    tenant_id       TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id              TEXT NOT NULL,
    -- What the tenant calls it, and what somebody asks for by name:
    -- "Werkgeversverklaring", "Employment confirmation", "Attestation".
    name            TEXT NOT NULL,
    -- The subject line of the draft. Merged like the body, because a subject
    -- that could not name the person would be the one line somebody edits every
    -- single time.
    subject         TEXT NOT NULL,
    -- The letter itself, plain text, with {{placeholders}} from the closed
    -- vocabulary (decision 2).
    body            TEXT NOT NULL,
    created_by      TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- Defence in depth: the store validates before writing, so a violation here
    -- is a bug in our code rather than user input.
    CONSTRAINT hr_letter_templates_name_present CHECK (length(name) > 0),
    CONSTRAINT hr_letter_templates_body_present CHECK (length(body) > 0)
);

-- One template per name: the agent resolves "the employment confirmation" by
-- the name a person typed, and two templates sharing one are two answers to the
-- question "which letter did you mean?".
CREATE UNIQUE INDEX hr_letter_templates_name_unique
    ON hr_letter_templates (tenant_id, lower(name));
