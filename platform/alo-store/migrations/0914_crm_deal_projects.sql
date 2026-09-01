-- A won Sales opportunity may become exactly one Projects engagement.
-- The tenant is repeated in both foreign keys so no relationship can cross
-- the workspace boundary, even when opaque identifiers are supplied directly.
CREATE TABLE crm_deal_projects (
    tenant_id      TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    deal_id        TEXT NOT NULL,
    project_id     TEXT NOT NULL,
    created_by     TEXT NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, deal_id),
    UNIQUE (tenant_id, project_id),
    FOREIGN KEY (tenant_id, deal_id)
        REFERENCES crm_deals(tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, project_id)
        REFERENCES task_projects(tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (created_by)
        REFERENCES users(id) ON DELETE RESTRICT
);

CREATE INDEX crm_deal_projects_by_project
    ON crm_deal_projects (tenant_id, project_id);
