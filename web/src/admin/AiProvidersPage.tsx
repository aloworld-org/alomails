// Admin — AI providers. A catalog of the backends alo can use, grouped into
// self-hosted (data never leaves your servers) and your-own-API-keys. Each card
// shows status, a Default badge, Manage, and an enable toggle; configuring one
// opens the provider modal. Matches the design-system admin screen.
import { useCallback, useEffect, useState } from "react";
import { KeyRound, Plus, Server } from "lucide-react";

import { strings } from "../i18n";
import { Card, Spinner, Toggle, cx } from "../ds";
import { useJmapClient } from "../jmap";
import type { AiProvider } from "../jmap";
import { CATALOG } from "./catalog";
import type { CatalogEntry } from "./catalog";
import { ProviderModal } from "./ProviderModal";
import styles from "./admin.module.css";

type StatusTone = "ok" | "muted";

function status(entry: CatalogEntry, p: AiProvider | undefined): { label: string; tone: StatusTone } {
  if (p?.enabled) return { label: strings.providerConnected, tone: "ok" };
  if (p !== undefined) {
    if (entry.needsKey && p.hasKey) return { label: strings.providerKeyAdded, tone: "muted" };
    return { label: strings.providerReady, tone: "muted" };
  }
  return { label: strings.providerNotConfigured, tone: "muted" };
}

export function AiProvidersPage() {
  const client = useJmapClient();
  const [providers, setProviders] = useState<AiProvider[] | null>(null);
  const [error, setError] = useState(false);
  const [editing, setEditing] = useState<CatalogEntry | null>(null);

  const load = useCallback(() => {
    setError(false);
    client
      .listProviders()
      .then(setProviders)
      .catch(() => setError(true));
  }, [client]);

  useEffect(load, [load]);

  const byKind = (kind: string): AiProvider | undefined => (providers ?? []).find((p) => p.kind === kind);
  const hasDefault = (providers ?? []).some((p) => p.isDefault && p.enabled);

  async function toggle(entry: CatalogEntry, p: AiProvider) {
    setProviders((prev) => (prev ?? []).map((x) => (x.id === p.id ? { ...x, enabled: !x.enabled } : x)));
    try {
      await client.upsertProvider({
        id: p.id,
        kind: p.kind,
        label: entry.name,
        baseUrl: p.baseUrl,
        model: p.model,
        enabled: !p.enabled,
      });
      if (!p.enabled && !hasDefault) await client.setDefaultProvider(p.id);
    } finally {
      load();
    }
  }

  function card(entry: CatalogEntry) {
    const p = byKind(entry.kind);
    const st = status(entry, p);
    const isDefault = p?.isDefault === true && p.enabled;
    return (
      <Card key={entry.kind} className={cx(styles.cardRow, isDefault && styles.cardDefault)}>
        <span className={styles.cardIcon}>
          {entry.group === "self" ? <Server size={20} strokeWidth={1.75} /> : <KeyRound size={20} strokeWidth={1.75} />}
        </span>
        <div className={styles.cardText}>
          <div className={styles.cardName}>
            <strong>{entry.name}</strong>
            {entry.builtIn === true && <span className={styles.builtIn}>{strings.builtInTag}</span>}
            <span className={cx(styles.pill, st.tone === "ok" ? styles.pillOk : styles.pillMuted)}>{st.label}</span>
            {isDefault && <span className={styles.defaultBadge}>{strings.adminDefaultBadge}</span>}
          </div>
          <p className={styles.cardDesc}>
            {p !== undefined && p.baseUrl.length > 0 ? `${p.baseUrl} · ${p.model}` : entry.description}
          </p>
        </div>
        <div className={styles.cardActions}>
          <button type="button" className={styles.ghost} onClick={() => setEditing(entry)}>
            {strings.adminManage}
          </button>
          {/* One unnamed switch per provider card: announced as "checkbox, not
              checked" whichever card it was on. It now says which provider it
              turns on, read but not drawn — the card's own heading is beside
              it. */}
          <Toggle
            checked={p?.enabled === true}
            disabled={p === undefined}
            onChange={() => p !== undefined && void toggle(entry, p)}
            label={strings.adminProviderEnabledFor(entry.name)}
            hideLabel
          />
        </div>
      </Card>
    );
  }

  const selfHosted = CATALOG.filter((c) => c.group === "self");
  const ownKeys = CATALOG.filter((c) => c.group === "keys");

  return (
    <div className={styles.page}>
      <header className={styles.pageHead}>
        <div>
          <h1>{strings.adminAiProviders}</h1>
          <p className={styles.pageIntro}>{strings.adminAiIntro}</p>
        </div>
        <button
          type="button"
          className={styles.primary}
          onClick={() => setEditing(CATALOG.find((c) => c.kind === "custom") ?? null)}
        >
          <Plus size={16} />
          <span>{strings.adminAddProvider}</span>
        </button>
      </header>

      {providers === null && !error && (
        <div className={styles.state}>
          <Spinner size={22} />
        </div>
      )}
      {error && (
        <div className={styles.state}>
          <p>{strings.adminProvidersError}</p>
          <button type="button" className={styles.textBtn} onClick={load}>
            {strings.mailRetry}
          </button>
        </div>
      )}

      {providers !== null && (
        <>
          <section className={styles.group}>
            <h2 className={styles.groupTitle}>{strings.adminAiSelfHosted}</h2>
            <p className={styles.groupHint}>{strings.adminAiSelfHostedHint}</p>
            <div className={styles.cardGrid}>{selfHosted.map(card)}</div>
          </section>

          <section className={styles.group}>
            <h2 className={styles.groupTitle}>{strings.adminAiOwnKeys}</h2>
            <p className={styles.groupHint}>{strings.adminAiOwnKeysHint}</p>
            <div className={styles.cardGrid}>{ownKeys.map(card)}</div>
          </section>

          <p className={styles.footnote}>{strings.adminAiFootnote}</p>
        </>
      )}

      {editing !== null && (
        <ProviderModal
          entry={editing}
          {...(() => {
            const p = byKind(editing.kind);
            return p !== undefined ? { provider: p } : {};
          })()}
          makeDefaultOnSave={!hasDefault}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            load();
          }}
        />
      )}
    </div>
  );
}
