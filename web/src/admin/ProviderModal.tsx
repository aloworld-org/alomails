// Configure one AI provider (admin). Matches the design-system "Connect …"
// modal: an API key with show/hide, the API endpoint, the model, and a
// Test-connection action that shows an inline "Connection verified" banner
// before you save. Saving enables the provider (and makes it the default when
// the tenant has none yet). A single model is used per provider for now.
import { useId, useState } from "react";
import type { FormEvent } from "react";
import { Check, Eye, EyeOff, KeyRound, RefreshCw, Server, X } from "lucide-react";

import { strings } from "../i18n";
import { Button, Chip, Field, IconButton, Input, Modal, Spinner } from "../ds";
import { useJmapClient } from "../jmap";
import type { AiProvider } from "../jmap";
import type { CatalogEntry } from "./catalog";
import styles from "./admin.module.css";

interface ProviderModalProps {
  entry: CatalogEntry;
  provider?: AiProvider;
  /** True when the tenant has no default yet, so a save should set this one. */
  makeDefaultOnSave: boolean;
  onClose: () => void;
  onSaved: () => void;
}

export function ProviderModal({ entry, provider, makeDefaultOnSave, onClose, onSaved }: ProviderModalProps) {
  const client = useJmapClient();
  const [baseUrl, setBaseUrl] = useState(provider?.baseUrl ?? entry.defaultBaseUrl);
  const [models, setModels] = useState<string[]>(
    provider?.model !== undefined && provider.model.length > 0
      ? provider.model.split(",").map((t) => t.trim()).filter((t) => t.length > 0)
      : [],
  );
  const [modelDraft, setModelDraft] = useState("");
  const [apiKey, setApiKey] = useState("");

  function addModel() {
    const m = modelDraft.trim();
    if (m.length === 0 || models.includes(m)) return;
    setModels([...models, m]);
    setModelDraft("");
  }
  const formId = useId();
  const [showKey, setShowKey] = useState(false);
  const [testing, setTesting] = useState(false);
  const [tested, setTested] = useState<{ ok: boolean; models: number } | "fail" | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function test() {
    if (baseUrl.trim().length === 0 || testing) return;
    setTesting(true);
    setTested(null);
    try {
      setTested(await client.testConnection(baseUrl.trim(), apiKey.trim()));
    } catch {
      setTested("fail");
    } finally {
      setTesting(false);
    }
  }

  async function save(e: FormEvent) {
    e.preventDefault();
    const draft = modelDraft.trim();
    const allModels = draft.length > 0 && !models.includes(draft) ? [...models, draft] : models;
    if (baseUrl.trim().length === 0 || allModels.length === 0) {
      setError(strings.providerRequired);
      return;
    }
    setSaving(true);
    setError(null);
    const id = provider?.id ?? crypto.randomUUID();
    try {
      await client.upsertProvider({
        id,
        kind: entry.kind,
        label: entry.name,
        baseUrl: baseUrl.trim(),
        // The first model is the one the AI features use; the rest are recorded.
        model: allModels.join(","),
        enabled: true,
        ...(apiKey.trim().length > 0 ? { apiKey: apiKey.trim() } : {}),
      });
      if (makeDefaultOnSave) await client.setDefaultProvider(id);
      onSaved();
    } catch {
      setError(strings.providerSaveError);
      setSaving(false);
    }
  }

  const verified = tested !== null && tested !== "fail" && tested.ok;
  const title = entry.needsKey
    ? strings.connectTitle(entry.name)
    : strings.configureTitle(entry.name);

  return (
    <Modal
      title={title}
      onClose={onClose}
      icon={entry.group === "self" ? <Server size={18} /> : <KeyRound size={18} />}
      actions={<IconButton label={strings.providerCancel} icon={<X size={18} />} onClick={onClose} />}
      footer={
        <>
          <Button
            variant="ghost"
            icon={testing ? <Spinner size={14} /> : <RefreshCw size={14} />}
            onClick={() => void test()}
            disabled={testing}
          >
            {testing ? strings.providerTesting : tested !== null ? strings.providerTestAgain : strings.providerTest}
          </Button>
          <div className={styles.footSpacer} />
          <Button variant="ghost" onClick={onClose}>
            {strings.providerCancel}
          </Button>
          <Button type="submit" form={formId} disabled={saving}>
            {saving ? <Spinner size={16} /> : strings.providerSave}
          </Button>
        </>
      }
    >
      {/* The form is in the body and Save is in the footer, so the two are
          joined by id rather than by nesting — and Enter in a field still
          saves. */}
      <form id={formId} className={styles.providerForm} onSubmit={save}>
        {entry.needsKey && (
          <Field label={strings.providerApiKey}>
            {(control) => (
              <div className={styles.keyRow}>
                <Input
                  {...control}
                  className={styles.keyRowGrow}
                  type={showKey ? "text" : "password"}
                  value={apiKey}
                  onChange={(e) => {
                    setApiKey(e.target.value);
                    setTested(null);
                  }}
                  placeholder={provider?.hasKey === true ? strings.providerApiKeyKept : "sk-…"}
                />
                <button
                  type="button"
                  className={styles.eyeBtn}
                  onClick={() => setShowKey((v) => !v)}
                  aria-label={showKey ? strings.providerHideKey : strings.providerShowKey}
                  aria-pressed={showKey}
                >
                  {showKey ? <EyeOff size={16} /> : <Eye size={16} />}
                </button>
              </div>
            )}
          </Field>
        )}

        <Field label={strings.providerBaseUrl}>
          {(control) => (
            <Input
              {...control}
              value={baseUrl}
              onChange={(e) => {
                setBaseUrl(e.target.value);
                setTested(null);
              }}
              placeholder="http://localhost:11434"
            />
          )}
        </Field>

        {/* Not a `ds/Field`: "Models" names a set of chips and the box that
            adds one, not a single control. The box carries its own name. */}
        <div className={styles.block}>
          <span className={styles.label}>{strings.providerModels}</span>
          <div className={styles.chipRow}>
            {models.map((m, i) => (
              <Chip
                key={m}
                tone={i === 0 ? "accent" : "neutral"}
                onRemove={() => setModels(models.filter((x) => x !== m))}
                removeLabel={strings.providerRemoveModel(m)}
              >
                {m}
              </Chip>
            ))}
            <input
              className={styles.chipInput}
              aria-label={strings.providerAddModel}
              value={modelDraft}
              onChange={(e) => setModelDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === ",") {
                  e.preventDefault();
                  addModel();
                }
              }}
              placeholder={
                models.length === 0
                  ? (entry.defaultModel ?? (entry.needsKey ? "gpt-4o-mini" : "llama3.2"))
                  : strings.providerModelPlaceholder
              }
            />
            <button type="button" className={styles.addChip} onClick={addModel}>
              {strings.providerAddModel}
            </button>
          </div>
        </div>

        {verified && (
          <div className={styles.verified}>
            <Check size={16} />
            <span>{strings.providerTestOk((tested as { models: number }).models)}</span>
          </div>
        )}
        {tested === "fail" && <div className={styles.failed}>{strings.providerTestFail}</div>}

        {error !== null && (
          <p className={styles.error} role="alert">
            {error}
          </p>
        )}
      </form>
    </Modal>
  );
}
