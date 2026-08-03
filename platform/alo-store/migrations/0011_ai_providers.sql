-- Multi-provider AI configuration (ADR 0011, extended for the admin console).
-- A tenant may configure several OpenAI-compatible backends (self-hosted Ollama,
-- OpenAI, a custom endpoint, or the built-in hosted default) and mark exactly
-- one enabled provider as the default the AI features use. Replaces the single
-- ai_config row from migration 0009 (no production data yet).
--
-- api_key is a secret: never returned to clients (only "has a key" is exposed)
-- and never logged. Providers are admin-set; the outbound URL is not a
-- user-editable field, so it cannot be an SSRF vector.
DROP TABLE IF EXISTS ai_config;

CREATE TABLE ai_providers (
    id         TEXT PRIMARY KEY,
    tenant_id  TEXT NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    -- 'ollama' | 'openai' | 'custom' | 'ficina' — a preset/label hint; all
    -- speak the OpenAI-compatible Chat Completions contract.
    kind       TEXT NOT NULL,
    label      TEXT NOT NULL DEFAULT '',
    base_url   TEXT NOT NULL DEFAULT '',
    model      TEXT NOT NULL DEFAULT '',
    api_key    TEXT,
    enabled    BOOLEAN NOT NULL DEFAULT FALSE,
    is_default BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX ai_providers_tenant ON ai_providers(tenant_id);
-- At most one default provider per tenant.
CREATE UNIQUE INDEX ai_providers_one_default ON ai_providers(tenant_id) WHERE is_default;
