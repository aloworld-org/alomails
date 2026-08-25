ALTER TABLE project_updates
    ADD COLUMN attachments JSONB NOT NULL DEFAULT '[]'::jsonb;
