-- Per-tenant AI inference configuration (ADR 0011). The AI layer is
-- model-agnostic and bring-your-own-backend: an operator points a tenant at an
-- OpenAI-compatible endpoint (Ollama/vLLM/hosted) and toggles it on. Set by the
-- operator (psql/CLI), like DKIM keys and aliases — never an untrusted-user
-- surface, so the outbound endpoint URL cannot be an SSRF vector.
--
-- api_key is a secret; like other secrets it lives here and is never returned
-- to clients or written to logs. One row per tenant.
CREATE TABLE ai_config (
    tenant_id  TEXT PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    base_url   TEXT NOT NULL DEFAULT '',
    model      TEXT NOT NULL DEFAULT '',
    api_key    TEXT,
    enabled    BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
