CREATE TABLE fin_report_schedules (
    tenant_id text NOT NULL,
    id text NOT NULL,
    report text NOT NULL CHECK (report IN ('pl','balance','aged_receivable','aged_payable','vat')),
    cadence text NOT NULL CHECK (cadence IN ('weekly','monthly','quarterly')),
    format text NOT NULL DEFAULT 'csv' CHECK (format IN ('csv')),
    recipient text NOT NULL,
    active boolean NOT NULL DEFAULT true,
    next_run_date date NOT NULL,
    last_run_at timestamptz,
    created_by text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, id)
);
CREATE INDEX fin_report_schedules_due_idx ON fin_report_schedules (next_run_date) WHERE active;
