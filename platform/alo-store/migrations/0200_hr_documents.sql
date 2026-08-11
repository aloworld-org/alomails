-- alo HR (ADR 0035, wave B6.02b): the papers a tenant keeps about a person —
-- the contract, the amendment that raised their pay, the letter confirming
-- their employment (docs/design/hr.md, "Routes" and "Data minimisation").
--
-- Three decisions are recorded here rather than assumed by whoever reads this
-- next.
--
-- 1. **The bytes are not here.** A document is a Drive node filed against a
--    person: one file tree, one version history, one blob store, one download
--    path. A second copy of file storage inside HR would be a second place a
--    contract can be, with different access rules — which is precisely the
--    failure this module exists to prevent.
--
-- 2. **The node must live in the tenant's HR area** (`drive_nodes.location_kind
--    = 'hr'`), and the store proves it on write. The HR area is a Drive
--    location whose read AND write permission is the HR role (or a tenant
--    admin), so the protection is Drive's own — a contract filed here cannot be
--    downloaded through `/drive/nodes/{id}/download` by a colleague, because
--    the location gate refuses them before the blob is ever opened. Filing a
--    node from somebody's personal files or from a Space would have made this
--    table a promise the file itself did not keep.
--
--    (`drive_nodes.location_kind` therefore has a third value from this
--    migration on — `'hr'` beside `'personal'` and `'space'`, which migration
--    0052's comment predates. The column has no CHECK, so this needs no DDL:
--    the vocabulary lives in `DriveLocation`, where the permission rule that
--    gives each word its meaning also lives.)
--
-- 3. **One node is filed against at most one person.** A contract belongs to
--    the person it names; a node filed twice would be one file with two
--    answers to "whose is this?", and the detach-and-refile that a mis-filing
--    actually needs stays possible either way.
--
-- Nothing here is a field value from the document. We do not read, parse or
-- summarise the contents of an employment contract; this table says which file
-- it is, what kind of paper it is, who filed it and when.

CREATE TABLE hr_documents (
    tenant_id    TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    id           TEXT NOT NULL,
    employee_id  TEXT NOT NULL,
    -- The Drive node holding the file. No FK: `drive_nodes` is keyed
    -- (tenant_id, id) and a composite FK would be right, but the node is also
    -- purge-able through Drive's own trash, and a purge must not be refused by
    -- a filing row — the store answers a missing node by omitting its name, and
    -- the filing stays as the record that a paper was once here.
    node_id      TEXT NOT NULL,
    -- The kind of paper. A closed vocabulary for the reason every other one in
    -- this suite is closed: a word no code knows is a category nothing can
    -- report on. Widening it is a design change (docs/design/hr.md).
    kind         TEXT NOT NULL,
    -- The filer's own words about WHICH paper this is ("addendum 4-day week"),
    -- never about the person. Bounded by the store.
    note         TEXT NOT NULL DEFAULT '',
    filed_by     TEXT NOT NULL,
    filed_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    -- CASCADE: a filing means nothing without the person, and the person is
    -- archived rather than deleted in ordinary operation.
    CONSTRAINT hr_documents_employee_fk FOREIGN KEY (tenant_id, employee_id)
        REFERENCES hr_employees (tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT hr_documents_kind CHECK (kind IN (
        'contract', 'amendment', 'letter', 'certificate', 'other'
    ))
);

-- "What is on this person's file?" — the only read this table has, newest
-- first.
CREATE INDEX hr_documents_by_employee
    ON hr_documents (tenant_id, employee_id, filed_at DESC);

-- One file, one person (decision 3 above).
CREATE UNIQUE INDEX hr_documents_node_unique
    ON hr_documents (tenant_id, node_id);
