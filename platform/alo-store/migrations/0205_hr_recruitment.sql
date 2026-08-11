-- alo HR (ADR 0035, wave B6.06a): job openings, the people who apply for them,
-- and what the people who met them wrote down (docs/design/hr.md,
-- "Recruitment-lite" and "The EU AI Act posture").
--
-- Five decisions are recorded here rather than left to whoever reads it next.
--
-- 1. **The stage vocabulary is closed and ordered, and lives in a CHECK.** Seven
--    words — applied, reviewing, interview, offer, hired, rejected, withdrawn.
--    The rejected alternative was configurable stages per opening, the shape
--    `crm_pipelines`/`crm_stages` has: a sales process genuinely differs by
--    product line, a hiring process for a company small enough to be replacing
--    Microsoft 365 with us has seven stages and always the same seven. Two more
--    tables, a seeding path and a migration, to express a preference nobody has
--    stated yet. It becomes two tables the day a tenant asks.
--
-- 2. **A CV is a Drive node id, and nothing here ever reads it.** No extracted
--    text column, no parsed-fields column, no score, no rank, no "fit". Annex
--    III point 4(a) of Regulation (EU) 2024/1689 classifies systems used to
--    analyse and filter job applications and to evaluate candidates as
--    high-risk, and the obligations apply from 2 August 2026. The absence of a
--    column to put a score in is the cheapest guarantee that nothing writes one.
--
-- 3. **`retain_until` is a column, not a job.** An unsuccessful applicant's data
--    has no employment-law retention behind it, so the row carries the date past
--    which nobody should still be holding it, and the hiring screen shows what
--    is past its date. The deletion is still a person pressing a button —
--    scheduled erasure is out of scope (docs/design/hr.md, "Out of scope"), and
--    a loop that deletes people unattended is not something we build. What the
--    module provides is the thing that remembers to ask.
--
-- 4. **Notes are rows against the applicant, with their author on them.** An
--    interview note is written by the person who was in the room, and "who wrote
--    this about me" is a question a candidate exercising a subject-access right
--    is entitled to have answered without a reconstruction.
--
-- 5. **Applicants are the one HR record that is deleted rather than archived.**
--    An employee record carries statutory retention in every member state and is
--    archived, never removed; an applicant who was not hired is the opposite
--    case, and a delete that leaves a tombstone would be the same personal data
--    under a different name. `ON DELETE CASCADE` from applicant to note makes
--    the erasure complete in one statement.
--
-- NUMBERING: the business track mints migrations from 0200 upward; everything
-- below 0200 belongs to the sites track.

CREATE TABLE hr_openings (
    tenant_id       TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id              TEXT NOT NULL,
    -- The role as it is advertised: "Backend engineer", "Magazijnmedewerker".
    title           TEXT NOT NULL,
    -- Which part of the company it is in. Free text, matching the employee
    -- record's `team` — a tenant's teams are their own vocabulary, and a table
    -- of them would be a second directory to keep in step with the first.
    team            TEXT NOT NULL DEFAULT '',
    -- Where the work is: a city, an office, "remote (EU)". Also free text, for
    -- the same reason.
    location        TEXT NOT NULL DEFAULT '',
    -- The terms on offer, from the same closed vocabulary an employment uses
    -- (`hr_employments.contract_kind`) — an opening that becomes a hire should
    -- not change words on the way.
    employment_kind TEXT NOT NULL DEFAULT 'permanent',
    -- draft → open → closed. Two transitions, both a person's act, both audited.
    -- `closed` is terminal: an opening that reopens is next year's opening, and
    -- pretending otherwise loses the dates of the first round.
    status          TEXT NOT NULL DEFAULT 'draft',
    -- The day it was published, and the day it was closed. Both NULL while it is
    -- a draft; the first is set by `publish`, the second by `close`.
    opened_on       DATE,
    closed_on       DATE,
    created_by      TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- Defence in depth: the store validates before writing, so a violation here
    -- is a bug in our code rather than user input.
    CONSTRAINT hr_openings_status CHECK (status IN ('draft', 'open', 'closed')),
    CONSTRAINT hr_openings_employment_kind CHECK (employment_kind IN (
        'permanent', 'fixed_term', 'part_time', 'apprentice', 'contractor', 'intern'
    )),
    CONSTRAINT hr_openings_title_present CHECK (length(title) > 0),
    -- A published opening has the day it was published from. A closed one has
    -- the day it closed, and `opened_on` only if it was ever published — a draft
    -- abandoned before the ad went out is closed without ever having been open.
    CONSTRAINT hr_openings_opened_when_open CHECK (
        status <> 'open' OR opened_on IS NOT NULL
    ),
    CONSTRAINT hr_openings_closed_when_closed CHECK (
        (status = 'closed') = (closed_on IS NOT NULL)
    ),
    CONSTRAINT hr_openings_closed_after_opened CHECK (
        opened_on IS NULL OR closed_on IS NULL OR closed_on >= opened_on
    )
);

-- "The openings this tenant has, newest first" — the hiring screen's only list.
CREATE INDEX hr_openings_by_status
    ON hr_openings (tenant_id, status, created_at DESC);

CREATE TABLE hr_applicants (
    tenant_id       TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id              TEXT NOT NULL,
    opening_id      TEXT NOT NULL,
    -- What they are called, as one field. Not given/family like an employee
    -- record: a candidate writes their own name on an application, and splitting
    -- it is a guess we would make wrongly for a large part of Europe and most of
    -- the world. It becomes an employee record — with the parts asked for — only
    -- if they are hired.
    name            TEXT NOT NULL,
    email           TEXT,
    phone           TEXT NOT NULL DEFAULT '',
    -- Where the application came from: "referral, Anna", "LinkedIn", the job
    -- board's name. The tenant's own words.
    source          TEXT NOT NULL DEFAULT '',
    -- The closed, ordered vocabulary (decision 1). Only `POST .../move` changes
    -- it, and only a person can call it.
    stage           TEXT NOT NULL DEFAULT 'applied',
    -- Their CV: a node in the tenant's HR area, whose read gate is the HR role
    -- itself. Never parsed (decision 2). NULL is ordinary — an application by
    -- telephone has no file.
    cv_node_id      TEXT,
    -- The day past which nobody should still be holding this (decision 3).
    retain_until    DATE NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, opening_id)
        REFERENCES hr_openings (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT hr_applicants_stage CHECK (stage IN (
        'applied', 'reviewing', 'interview', 'offer', 'hired', 'rejected', 'withdrawn'
    )),
    CONSTRAINT hr_applicants_name_present CHECK (length(name) > 0)
);

-- "The pipeline for this opening" — the board's read, and the only one.
CREATE INDEX hr_applicants_by_opening
    ON hr_applicants (tenant_id, opening_id, stage, created_at);

-- "What is past its retention date" — the question the hiring screen asks so a
-- person can answer it (decision 3).
CREATE INDEX hr_applicants_by_retention
    ON hr_applicants (tenant_id, retain_until);

CREATE TABLE hr_applicant_notes (
    tenant_id       TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id              TEXT NOT NULL,
    applicant_id    TEXT NOT NULL,
    -- Who was in the room (decision 4).
    author_user_id  TEXT NOT NULL,
    body            TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, applicant_id)
        REFERENCES hr_applicants (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT hr_applicant_notes_body_present CHECK (length(body) > 0)
);

-- "This candidate's notes, newest first" — the only read this table serves.
CREATE INDEX hr_applicant_notes_by_applicant
    ON hr_applicant_notes (tenant_id, applicant_id, created_at DESC);
