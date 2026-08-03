-- Per-folder delegation (ADR 0017, Outlook parity): optionally restrict a
-- delegate's access to specific folders of the owner's mailbox. A grant with NO
-- rows here is whole-mailbox (the default, unchanged behaviour); one or more
-- rows restrict the delegate to exactly those mailboxes — every other folder of
-- the owner is then invisible and untouchable to that delegate.
--
-- Cascades with the grant: revoking the delegate (deleting the account_delegates
-- row) removes its folder restrictions too. Tenant-scoped for query hygiene,
-- though the grant it hangs off is already tenant-bound.
CREATE TABLE delegate_folders (
    tenant_id   TEXT NOT NULL,
    owner_id    TEXT NOT NULL,
    delegate_id TEXT NOT NULL,
    mailbox_id  TEXT NOT NULL,
    PRIMARY KEY (owner_id, delegate_id, mailbox_id),
    FOREIGN KEY (owner_id, delegate_id)
        REFERENCES account_delegates (owner_id, delegate_id) ON DELETE CASCADE
);

CREATE INDEX delegate_folders_lookup ON delegate_folders (owner_id, delegate_id);
