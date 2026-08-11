-- alo HR (ADR 0035, wave B6.10): the fact that somebody drew the payroll file
-- (docs/design/hr.md, "Payroll export").
--
-- This table holds **no payroll data**. It is the receipt for a read: which
-- period, in which column mapping, how many people were on it, who asked and
-- when. The figures themselves are folded from the employees, their employments
-- and their approved leave every time the file is drawn, and are never stored a
-- second time here.
--
-- Three decisions are recorded here rather than left to whoever reads it next.
--
-- 1. **The export is a POST because this read deserves a line.** Every other
--    export in the suite is a GET with a `.csv` twin; the audit trail records
--    mutations only. This one read returns every employee's pay, national
--    identifier and bank account in one response, and "who downloaded the
--    payroll file, and when" is a question a works council, a data-protection
--    officer and a fraud investigation all ask. Making the draw a row is the
--    smallest honest way to get it into the log that already exists — the
--    business-mutation middleware files `hr.payroll_export.create` for the same
--    request that inserts here.
--
-- 2. **Nothing in this table is personal data about the people on the file.**
--    `drawn_by` is the user who asked, which every audit row already carries;
--    there is no employee id, no name, no amount. A row here can be kept for as
--    long as the audit trail is kept without keeping anybody's salary with it.
--
-- 3. **`line_count` is stored because it is the fact that ages.** The same
--    period drawn twice a year apart can legitimately produce different files
--    (a leaver, a late-approved claim), and "the March file had 14 people on it"
--    is what makes a re-draw explicable rather than suspicious. It is a count,
--    not a figure.
--
-- WHAT IS DELIBERATELY NOT HERE: the file. We do not keep a copy of a document
-- carrying every employee's pay and IBAN in a table we would then have to
-- defend; the draw is reproducible from the records, and the receipt says it
-- happened.
--
-- NUMBERING: the business track mints migrations from 0200 upward; everything
-- below 0200 belongs to the sites track.

CREATE TABLE hr_payroll_exports (
    tenant_id       TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id              TEXT NOT NULL,
    -- The period the file covered, both days included.
    from_day        DATE NOT NULL,
    to_day          DATE NOT NULL,
    -- Which column mapping rendered it: the file a bureau received is only
    -- reproducible with the layout it was drawn in.
    mapping_key     TEXT NOT NULL,
    -- How many people were on it (decision 3).
    line_count      INTEGER NOT NULL,
    -- The user who drew it.
    drawn_by        TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    CONSTRAINT hr_payroll_exports_period_ordered CHECK (to_day >= from_day),
    CONSTRAINT hr_payroll_exports_mapping_present CHECK (length(mapping_key) > 0),
    CONSTRAINT hr_payroll_exports_lines_counted CHECK (line_count >= 0)
);

-- The receipts, newest first: "when was this quarter last drawn, and by whom".
CREATE INDEX hr_payroll_exports_recent
    ON hr_payroll_exports (tenant_id, created_at DESC, id);
