-- DMARC aggregate-report events (RFC 7489 §7.2): one row per inbound
-- message for which a DMARC policy was discovered and evaluated at the
-- MX. Host-level operational data (no tenant, no message content) —
-- exactly the fields the domain owner receives in the aggregate
-- report. Rows are deleted once reported.
CREATE TABLE dmarc_report_events (
    id            BIGSERIAL PRIMARY KEY,
    -- RFC 5322 From domain the policy was evaluated for (lowercased).
    from_domain   TEXT NOT NULL,
    -- Connecting client IP the messages arrived from.
    source_ip     TEXT NOT NULL,
    -- Applied disposition: none | quarantine | reject.
    disposition   TEXT NOT NULL,
    -- DMARC alignment outcomes (§3.1).
    dkim_aligned  BOOLEAN NOT NULL,
    spf_aligned   BOOLEAN NOT NULL,
    evaluated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- The report sweep scans by window then domain.
CREATE INDEX dmarc_report_events_window
    ON dmarc_report_events (evaluated_at, from_domain);
