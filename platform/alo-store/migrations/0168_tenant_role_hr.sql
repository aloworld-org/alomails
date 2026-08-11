-- alo HR (ADR 0035, wave B6.02a): the second scoped role
-- (docs/design/hr.md, "The HR role").
--
-- Migration 0149 said, in its own words, that "the second role widens this
-- table by a value in the CHECK and the gates by a word", and named B6's HR
-- role as the likely one. This file takes it at its word.
--
-- WHY A WIDENED CHECK IS APPEND-ONLY. Dropping and re-adding a CHECK with a
-- larger accepted set loses no row, no column and no grant: every value that
-- was legal stays legal. It is expand-only in the sense the constitution means,
-- and it is the only way PostgreSQL lets a CHECK grow. The re-add is NOT
-- VALID-free on purpose — the table is small and every existing row already
-- satisfies the wider set, so the validating scan is free.
--
-- WHAT THE ROLE MEANS is decided by the gates, not here: `require_hr` accepts a
-- tenant admin OR this role. The role only ever ADDS — an HR holder who is also
-- an ordinary employee keeps every ordinary capability they had, and the
-- accountant's gate is untouched. An accountant, specifically, may NOT read
-- /hr/*: "the books and none of the mail" would be a strange place to find
-- everybody's contract and home address.

ALTER TABLE tenant_user_roles
    DROP CONSTRAINT tenant_user_roles_known;

ALTER TABLE tenant_user_roles
    ADD CONSTRAINT tenant_user_roles_known
        CHECK (role IN ('accountant', 'hr'));
