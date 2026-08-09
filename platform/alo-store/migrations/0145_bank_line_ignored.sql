-- alo Finance (ADR 0035, wave B4.09c): why a staged line is not ours to book
-- (docs/design/finance.md, "Matching is three stages", stage 3).
--
-- A LINE LEAVES THE PILE IN ONE OF TWO WAYS. Either a person says which
-- document it settles (`bank_matches`, migration 0143) or they say it settles
-- nothing of ours — a bank charge, an own transfer between two of the tenant's
-- accounts, a payment already booked by hand months ago. The second way needs
-- no row of its own: it is the line's own `status`. What it does need is the
-- sentence that goes with it, because "ignored" with no reason is the state a
-- bookkeeper cannot audit, cannot hand over and cannot undo with confidence six
-- months later.
--
-- ONE COLUMN, NOT THREE. Who ignored the line and when are already recorded:
-- every mutating `/finance/*` route writes an audit entry naming the actor, the
-- act and the line (B2.13), and duplicating that here would be a second answer
-- to a question that already has one. What the audit log cannot hold is the
-- reason, because a reason is part of the record rather than part of its
-- history — it belongs beside the line a person is reading.
--
-- THE REASON AND THE STATUS MOVE TOGETHER. The CHECK below is the invariant
-- that keeps "un-ignore" honest: taking the line back into the pile clears the
-- sentence, so a line that is unmatched never carries a stale explanation of a
-- decision somebody already took back.
--
-- The business track mints migrations in the 01xx block; the sites track
-- continues in 00xx (docs/autonomy/STATE.md, 2026-08-06).

ALTER TABLE bank_lines
    ADD COLUMN ignored_reason TEXT NOT NULL DEFAULT '';

ALTER TABLE bank_lines
    ADD CONSTRAINT bank_lines_ignored_reason_shape
        CHECK (char_length(ignored_reason) <= 200);

-- Only an ignored line has a reason, and an ignored line always has one: the
-- store refuses a blank reason before it writes, and this is the same rule
-- stated where no caller can get around it.
ALTER TABLE bank_lines
    ADD CONSTRAINT bank_lines_ignored_reason_with_status
        CHECK ((status = 'ignored') = (ignored_reason <> ''));
