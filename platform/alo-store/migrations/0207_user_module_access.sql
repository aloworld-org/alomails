-- Which apps a person may open (admin console → a user's apps).
--
-- WHY DENIALS AND NOT GRANTS. The obvious table is "user X may open module Y",
-- and it is the wrong way round for this product. Every existing account would
-- need a backfill row per module on the day this shipped, every new module
-- would need another backfill for every user, and the failure mode of a missed
-- backfill is a person who silently loses an app they had yesterday. Stored as
-- denials, the empty table means what it says — everybody can open everything,
-- which is exactly true today — and a new module is available the moment it
-- exists. The admin still sees checkboxes that read "has access"; the UI shows
-- the complement, because that is the sentence an administrator thinks in.
--
-- The cost is honest and worth naming: a new module is open to everyone until
-- somebody turns it off. For a suite of business apps that is the right
-- default. It would be the wrong default for anything sold per seat per app,
-- and if alo ever prices that way this table's polarity is the thing to
-- revisit — not the surfaces around it, which read a set either way.
--
-- WHY NOT A ROLE. `tenant_user_roles` answers "what may this person do" for
-- cross-cutting jobs — the books, the workforce — and the roles are a closed
-- set with gates that name them in words. This answers a flatter question:
-- which of the apps in the rail was this person given. A tenant with fifty
-- people has fifty different answers and no two of them are a job title, so
-- modelling it as roles would mint a role per combination.
--
-- WHAT A ROW DOES NOT DO. It never grants. Somebody denied nothing still needs
-- the role or the Space membership that the module itself requires, so this
-- narrows and never widens — the same rule `tenant_user_roles` states for the
-- HR role, in the other direction.
--
-- TENANCY. `tenant_id` is carried beside `user_id` even though `users.id` is
-- globally unique, so every read is tenant-bound by construction, and the write
-- path proves the user belongs to the denying tenant before it writes a row.

CREATE TABLE tenant_user_module_denials (
    tenant_id TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    user_id   TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    module    TEXT NOT NULL,
    -- Provenance, as with a role grant: taking an app away from somebody is a
    -- decision an auditor asks about, and "who and when" is the answer.
    denied_by TEXT NOT NULL,
    denied_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, user_id, module),
    -- A module this build has no gate for is a denial that silently does
    -- nothing, which is worse than a refusal — the console would show the app
    -- as switched off while every route still answered. The set is the rail's
    -- own ids, and the API gate maps its path prefixes onto them.
    --
    -- Mail and Home are deliberately absent and cannot be denied: `/jmap`
    -- carries the session, blob upload and the event stream that every other
    -- surface depends on, so a denial there would not mean "no mail app", it
    -- would mean a broken account. If mail ever needs to be withheld it needs
    -- its own answer, not a row here.
    CONSTRAINT tenant_user_module_denials_known
        CHECK (module IN (
            'agenda', 'billing', 'chat', 'crm', 'drive', 'finance', 'hr',
            'insights', 'inventory', 'meet', 'projects', 'sites', 'tasks'
        ))
);

-- "What is this person denied?" — asked once when a session is minted and once
-- per gated request, so it is the read that has to be cheap. The primary key
-- already serves it; this index serves the console's other question, "who is
-- denied this app?", when a module is being switched off across a tenant.
CREATE INDEX tenant_user_module_denials_by_module
    ON tenant_user_module_denials (tenant_id, module);
