-- Mailbox counters must never go negative: a drift bug then fails its
-- transaction loudly instead of surfacing a corrupt (negative) count to
-- clients. Paired with the FOR UPDATE locking on the counter paths so
-- legitimate operations can never trip it.
ALTER TABLE mailboxes
    ADD CONSTRAINT mailboxes_counts_nonneg
    CHECK (total_messages >= 0 AND unread_messages >= 0);

-- Threading joins on the References/In-Reply-To message-ids, never on
-- the base subject, so the threads(subject_base) index was dead. Drop
-- it; the subject_base column stays as the thread's display subject.
DROP INDEX IF EXISTS threads_tenant_subject;
