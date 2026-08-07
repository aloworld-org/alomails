-- alo Sites (ADR 0036): contact forms and their submissions. A form is the
-- store object a `contact_form` section points at by id (the section stores
-- `form_id`); submissions are what visitors post through the public
-- `POST /f/:form_id` endpoint (a later slice — nothing public writes yet).
-- Tenant-scoped and cascading through the site (tenants -> sites ->
-- site_forms -> site_form_submissions). Per the privacy model in
-- docs/design/sites.md, a submission stores ONLY the posted fields — never
-- the visitor's IP address or user agent, and there are no columns for them.

CREATE TABLE site_forms (
    tenant_id  TEXT NOT NULL,
    site_id    TEXT NOT NULL,
    id         TEXT NOT NULL,
    -- Owner-facing label for the submissions UI ("Contact", "Careers").
    name       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, site_id) REFERENCES sites (tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX site_forms_by_site ON site_forms (tenant_id, site_id);

-- The public submit endpoint holds only a bare form id (the token a
-- `contact_form` section carries) and must resolve it to its tenant + site in
-- one indexed lookup. Ids are 128-bit random, so global uniqueness holds by
-- construction; this index makes it a constraint and the lookup fast.
CREATE UNIQUE INDEX site_forms_id_unique ON site_forms (id);

CREATE TABLE site_form_submissions (
    tenant_id    TEXT NOT NULL,
    form_id      TEXT NOT NULL,
    id           TEXT NOT NULL,
    -- The posted fields, size-capped in the store. No IP, no user agent —
    -- adding such a column is a privacy-model change requiring design review.
    sender_name  TEXT NOT NULL,
    sender_email TEXT NOT NULL,
    message      TEXT NOT NULL,
    -- Owner workflow flag ("dealt with"), toggled from the submissions UI.
    handled      BOOLEAN NOT NULL DEFAULT false,
    received_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id),
    FOREIGN KEY (tenant_id, form_id) REFERENCES site_forms (tenant_id, id) ON DELETE CASCADE
);

-- The submissions UI lists newest-first per form.
CREATE INDEX site_form_submissions_by_form
    ON site_form_submissions (tenant_id, form_id, received_at DESC);
