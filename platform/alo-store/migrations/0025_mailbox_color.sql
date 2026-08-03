-- Mailbox color (label chrome): an optional display color for a mailbox/label,
-- so custom folders can be color-coded in the UI like Gmail/Outlook categories.
-- Stored as a "#rrggbb" hex string (validated at the API boundary); NULL means
-- no color (the UI falls back to a neutral swatch).
ALTER TABLE mailboxes ADD COLUMN color TEXT;
