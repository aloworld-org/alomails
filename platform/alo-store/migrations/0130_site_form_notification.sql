-- alo Sites (ADR 0036): owner notification for contact-form submissions.
-- A NULL notified_at marks a submission the owner has not been told about
-- yet; the notifier sweep claims such rows (setting notified_at) and
-- delivers an internal message to the site owner's inbox. Rows that
-- predate the notifier are marked as already handled so a deploy never
-- floods owners with notifications for old submissions.

ALTER TABLE site_form_submissions
    ADD COLUMN notified_at TIMESTAMPTZ;

UPDATE site_form_submissions
   SET notified_at = received_at
 WHERE notified_at IS NULL;

-- The sweep scans only pending rows, oldest first (its claim orders by
-- received_at, id — the index carries both).
CREATE INDEX site_form_submissions_unnotified
    ON site_form_submissions (received_at, id)
    WHERE notified_at IS NULL;
