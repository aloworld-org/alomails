-- Ficina Transfer now streams share files under their own object key
-- (`<tenant>/share/<id>`) rather than storing them content-addressed in the
-- blobs table. Rename the column to reflect that, and drop the now-unused blob
-- lookup index (the sweeper deletes by the DELETE…RETURNING key, and resolution
-- is by token_hash primary key).
DROP INDEX IF EXISTS file_shares_blob_idx;
ALTER TABLE file_shares RENAME COLUMN blob_id TO object_key;
