-- Inviting somebody into the workspace, instead of choosing a password for them.
--
-- WHAT THIS REPLACES. Until now an admin created a colleague by typing an
-- email and a password into a form, and then told them that password over
-- WhatsApp. Three things were wrong with it and only one is obvious: the
-- password crosses a channel nobody controls; the admin knows it afterwards,
-- for ever, with nothing forcing a change; and — the quiet one — the account
-- never captured a recovery address, so `/reset/*` could not help the person
-- who forgot it. Their only route back was to ask the admin, who would set
-- another password and send it the same way.
--
-- So an invited account sets its own credential and its own recovery address
-- in one act, and the admin learns neither.
--
-- WHY A SEPARATE TABLE FROM `site_editor_invites`. That one invites somebody
-- to edit one site, and its lookup joins through the grant to name the site in
-- the acceptance page. This one invites somebody into the workspace itself and
-- has no grant to join. Sharing a table would mean a nullable site column and
-- a query that means two different things depending on whether it is null,
-- which is how one table becomes two tables that cannot be told apart. The
-- shape is copied deliberately; the rows are not mixed.
--
-- THE TOKEN IS NEVER STORED. Only its hash, exactly as password reset does, so
-- a leaked database backup does not hand somebody every outstanding invitation
-- to a workspace.
--
-- EXPIRY IS NOT OPTIONAL. An invitation is a credential-shaped thing sitting
-- in a mailbox; one that never expires is a permanent key to an account that
-- may never have been claimed. Seven days, and an admin can always send
-- another.

CREATE TABLE user_invites (
    token_hash TEXT PRIMARY KEY,
    tenant_id  TEXT NOT NULL REFERENCES tenants (id) ON DELETE CASCADE,
    user_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- The address the invitation was sent to, which is also the username the
    -- credential will be installed under. Carried here so acceptance needs no
    -- second lookup and cannot install a credential for a different address
    -- than the one that was invited.
    email      TEXT NOT NULL,
    -- Provenance: who invited them. An account that appeared in a workspace is
    -- a thing an auditor asks the origin of.
    invited_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    -- Set when spent. The row is kept rather than deleted so "when did this
    -- person accept?" stays answerable, and so a spent token is refused by the
    -- same query that refuses an unknown one.
    accepted_at TIMESTAMPTZ
);

-- "Does this person have an invitation outstanding?" — the admin console asks
-- it per user when it draws the list, so it is the read that has to be cheap.
CREATE INDEX user_invites_by_user
    ON user_invites (tenant_id, user_id)
    WHERE accepted_at IS NULL;
