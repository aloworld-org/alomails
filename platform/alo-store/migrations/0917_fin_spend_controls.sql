CREATE TABLE fin_spend_policies (
    tenant_id TEXT PRIMARY KEY REFERENCES tenants (id) ON DELETE CASCADE,
    receipt_required_above_cents BIGINT,
    project_required_above_cents BIGINT,
    second_approval_above_cents BIGINT,
    updated_by TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT fin_spend_receipt_threshold CHECK (receipt_required_above_cents IS NULL OR receipt_required_above_cents >= 0),
    CONSTRAINT fin_spend_project_threshold CHECK (project_required_above_cents IS NULL OR project_required_above_cents >= 0),
    CONSTRAINT fin_spend_second_threshold CHECK (second_approval_above_cents IS NULL OR second_approval_above_cents >= 0)
);

CREATE TABLE fin_expense_approvals (
    tenant_id TEXT NOT NULL,
    expense_id TEXT NOT NULL,
    approver_id TEXT NOT NULL,
    note TEXT NOT NULL DEFAULT '',
    approved_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, expense_id, approver_id),
    CONSTRAINT fin_expense_approvals_expense_fk FOREIGN KEY (tenant_id, expense_id)
        REFERENCES fin_expenses (tenant_id, id) ON DELETE CASCADE
);

CREATE INDEX fin_expense_approvals_claim_idx
    ON fin_expense_approvals (tenant_id, expense_id, approved_at);
