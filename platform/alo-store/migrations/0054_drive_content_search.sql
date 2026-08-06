-- Drive content search (ADR 0029): index the *text* of a file so workspace
-- search can match inside it, not only on its name. `content` is a full-text
-- vector built at write time from a node's bytes when they are text-extractable
-- (a plain-text file, or an alo Doc's BlockNote JSON); binary formats
-- (docx/xlsx/pdf) are left NULL until a text-extraction pipeline lands, and stay
-- name-searchable meanwhile. Additive/expand-only: existing rows are NULL
-- (name-searchable) and gain a content index the next time they are saved.
ALTER TABLE drive_nodes ADD COLUMN content tsvector;
CREATE INDEX drive_nodes_content ON drive_nodes USING GIN(content);
