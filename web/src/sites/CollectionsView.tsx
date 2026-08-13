// The recognition-first CMS collection workspace. It follows Webflow CMS and
// Contentful's familiar source → field mapping → card preview flow, while the
// source of truth stays an alo Base the caller already has permission to read.
import { useCallback, useEffect, useMemo, useState } from "react";
import { ArrowLeft, Database, ExternalLink, Link2, Plus, Rows3, Unplug } from "lucide-react";
import { Link, useNavigate, useParams } from "react-router-dom";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import { EmptyState, ErrorBanner } from "./parts";
import type {
  SiteCollection,
  SiteCollectionDraft,
  SiteCollectionFieldMapping,
  SiteCollectionPreview,
  SiteCollectionSource,
  SiteCollectionSourceField,
  SiteCollectionSourceTable,
} from "./types";
import styles from "./SitesModule.module.css";

const emptyMapping = (): SiteCollectionFieldMapping => ({
  title: "",
  slug: null,
  summary: null,
  body: null,
  image: null,
  link: null,
  publishedAt: null,
});

const emptyDraft = (): SiteCollectionDraft => ({
  name: "",
  baseNodeId: "",
  baseTableId: "",
  mapping: emptyMapping(),
});

function draftFrom(collection: SiteCollection): SiteCollectionDraft {
  return {
    name: collection.name,
    baseNodeId: collection.baseNodeId,
    baseTableId: collection.baseTableId,
    mapping: { ...collection.mapping },
  };
}

function mappingFor(table: SiteCollectionSourceTable): SiteCollectionFieldMapping {
  return {
    ...emptyMapping(),
    title: table.fields.find((field) => field.type === "text")?.id ?? "",
  };
}

function draftForFirstSource(sources: SiteCollectionSource[]): SiteCollectionDraft {
  const source = sources[0];
  const table = source?.tables[0];
  if (source === undefined || table === undefined) return emptyDraft();
  return {
    name: table.name,
    baseNodeId: source.nodeId,
    baseTableId: table.id,
    mapping: mappingFor(table),
  };
}

interface MappingFieldProps {
  label: string;
  value: string | null;
  fields: SiteCollectionSourceField[];
  required?: boolean | undefined;
  onChange: (value: string | null) => void;
}

function MappingField({ label, value, fields, required = false, onChange }: MappingFieldProps) {
  return (
    <label className={styles.collectionMappingField}>
      <span>
        {label}
        {!required && <small>{strings.sitesCollectionOptional}</small>}
      </span>
      <select
        className={styles.input}
        value={value ?? ""}
        onChange={(event) => onChange(event.target.value === "" ? null : event.target.value)}
      >
        {!required && <option value="">{strings.sitesCollectionNotMapped}</option>}
        {required && fields.length === 0 && (
          <option value="">{strings.sitesCollectionNoCompatibleField}</option>
        )}
        {fields.map((field) => (
          <option key={field.id} value={field.id}>
            {field.name}
          </option>
        ))}
      </select>
    </label>
  );
}

export function CollectionsView() {
  const { siteId = "" } = useParams();
  const navigate = useNavigate();
  const api = useSitesApi();
  const [collections, setCollections] = useState<SiteCollection[]>([]);
  const [sources, setSources] = useState<SiteCollectionSource[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draft, setDraft] = useState<SiteCollectionDraft>(emptyDraft);
  const [preview, setPreview] = useState<SiteCollectionPreview | null>(null);
  const [loading, setLoading] = useState(true);
  const [previewBusy, setPreviewBusy] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [disconnectArmed, setDisconnectArmed] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [connected, available] = await Promise.all([
        api.collections(siteId),
        api.collectionSources(),
      ]);
      setCollections(connected);
      setSources(available);
      const selected = connected[0];
      setSelectedId(selected?.id ?? null);
      setDraft(selected === undefined ? draftForFirstSource(available) : draftFrom(selected));
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesCollectionsLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, siteId]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    setDisconnectArmed(false);
    setPreview(null);
    setPreviewError(null);
    if (selectedId === null) return;
    let cancelled = false;
    setPreviewBusy(true);
    void api.collectionPreview(siteId, selectedId).then(
      (result) => {
        if (!cancelled) setPreview(result);
      },
      (reason: unknown) => {
        if (!cancelled) {
          setPreviewError(sitesMessage(reason, strings.sitesCollectionPreviewFailed));
        }
      },
    ).finally(() => {
      if (!cancelled) setPreviewBusy(false);
    });
    return () => {
      cancelled = true;
    };
  }, [api, selectedId, siteId]);

  const source = sources.find((candidate) => candidate.nodeId === draft.baseNodeId);
  const table = source?.tables.find((candidate) => candidate.id === draft.baseTableId);
  const textFields = table?.fields.filter((field) => field.type === "text") ?? [];
  const imageFields = table?.fields.filter((field) => field.type === "attachment") ?? [];
  const dateFields = table?.fields.filter((field) => field.type === "date") ?? [];
  const canSave =
    draft.name.trim() !== "" &&
    draft.baseNodeId !== "" &&
    draft.baseTableId !== "" &&
    draft.mapping.title !== "";
  const selectedCollection = collections.find((collection) => collection.id === selectedId);

  const sourceLabel = useMemo(() => {
    if (source === undefined || table === undefined) return strings.sitesCollectionSourceUnavailable;
    return strings.sitesCollectionConnectedTo(source.name, table.name);
  }, [source, table]);

  function selectCollection(collection: SiteCollection) {
    setSelectedId(collection.id);
    setDraft(draftFrom(collection));
    setError(null);
  }

  function startConnection() {
    setSelectedId(null);
    setDraft(draftForFirstSource(sources));
    setPreview(null);
    setPreviewError(null);
    setDisconnectArmed(false);
  }

  function chooseSource(nodeId: string) {
    const nextSource = sources.find((candidate) => candidate.nodeId === nodeId);
    const nextTable = nextSource?.tables[0];
    setDraft(
      nextSource === undefined || nextTable === undefined
        ? emptyDraft()
        : {
            name: nextTable.name,
            baseNodeId: nextSource.nodeId,
            baseTableId: nextTable.id,
            mapping: mappingFor(nextTable),
          },
    );
  }

  function chooseTable(tableId: string) {
    const nextTable = source?.tables.find((candidate) => candidate.id === tableId);
    if (nextTable === undefined) return;
    setDraft((current) => ({
      ...current,
      name: selectedId === null ? nextTable.name : current.name,
      baseTableId: nextTable.id,
      mapping: mappingFor(nextTable),
    }));
  }

  function mapField(key: keyof SiteCollectionFieldMapping, value: string | null) {
    setDraft((current) => ({
      ...current,
      mapping: { ...current.mapping, [key]: value ?? (key === "title" ? "" : null) },
    }));
  }

  async function save() {
    if (!canSave) return;
    setBusy(true);
    setError(null);
    try {
      const stored = selectedId === null
        ? await api.createCollection(siteId, draft)
        : await api.updateCollection(siteId, selectedId, draft);
      const next = selectedId === null
        ? [...collections, stored]
        : collections.map((collection) => collection.id === stored.id ? stored : collection);
      setCollections(next);
      setSelectedId(stored.id);
      setDraft(draftFrom(stored));
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesCollectionSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  async function disconnect() {
    if (selectedId === null) return;
    if (!disconnectArmed) {
      setDisconnectArmed(true);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await api.disconnectCollection(siteId, selectedId);
      const remaining = collections.filter((collection) => collection.id !== selectedId);
      setCollections(remaining);
      const next = remaining[0];
      setSelectedId(next?.id ?? null);
      setDraft(next === undefined ? draftForFirstSource(sources) : draftFrom(next));
      setPreview(null);
      setDisconnectArmed(false);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesCollectionDisconnectFailed));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <Link to=".." relative="path" className={styles.backLink}>
          <ArrowLeft size="var(--icon-size-inline)" aria-hidden="true" />
          {strings.sitesBack}
        </Link>
        <div>
          <h1 className={styles.title}>{strings.sitesCollections}</h1>
          <p className={styles.collectionPageHint}>{strings.sitesCollectionsHint}</p>
        </div>
        {!loading && sources.length > 0 && (
          <Button icon={<Plus size="var(--icon-size-inline)" />} onClick={startConnection}>
            {strings.sitesConnectTable}
          </Button>
        )}
      </header>

      {error !== null && <ErrorBanner message={error} />}
      {loading ? (
        <div className={styles.collectionLoading} role="status">
          <Spinner size={20} />
          <span>{strings.sitesCollectionsLoading}</span>
        </div>
      ) : sources.length === 0 ? (
        <EmptyState
          Icon={Database}
          title={strings.sitesCollectionNoBasesTitle}
          body={strings.sitesCollectionNoBasesBody}
          cta={strings.sitesCollectionOpenDrive}
          onCta={() => navigate("/drive")}
        />
      ) : (
        <div className={styles.collectionWorkspace}>
          <aside className={styles.collectionList} aria-label={strings.sitesCollections}>
            {collections.length === 0 && (
              <div className={styles.collectionListEmpty}>
                <Database aria-hidden="true" />
                <strong>{strings.sitesCollectionEmptyTitle}</strong>
                <span>{strings.sitesCollectionEmptyBody}</span>
              </div>
            )}
            {collections.map((collection) => {
              const connectedSource = sources.find(
                (candidate) => candidate.nodeId === collection.baseNodeId,
              );
              const connectedTable = connectedSource?.tables.find(
                (candidate) => candidate.id === collection.baseTableId,
              );
              const binding = connectedSource === undefined || connectedTable === undefined
                ? strings.sitesCollectionSourceUnavailable
                : strings.sitesCollectionConnectedTo(connectedSource.name, connectedTable.name);
              return (
              <button
                key={collection.id}
                type="button"
                className={`${styles.collectionListItem} ${
                  collection.id === selectedId ? styles.collectionListItemActive : ""
                }`}
                aria-pressed={collection.id === selectedId}
                onClick={() => selectCollection(collection)}
              >
                <Rows3 aria-hidden="true" />
                <span>
                  <strong>{collection.name}</strong>
                  <small>{binding}</small>
                </span>
              </button>
              );
            })}
          </aside>

          <div className={styles.collectionEditor}>
            <section className={styles.collectionSetup} aria-labelledby="collection-setup-title">
              <div className={styles.collectionPanelHead}>
                <div>
                  <h2 id="collection-setup-title">
                    {selectedCollection === undefined
                      ? strings.sitesConnectTable
                      : strings.sitesCollectionEdit(selectedCollection.name)}
                  </h2>
                  <p>{sourceLabel}</p>
                </div>
                <Button
                  variant="ghost"
                  icon={<ExternalLink size="var(--icon-size-inline)" />}
                  onClick={() => navigate("/drive")}
                >
                  {strings.sitesCollectionOpenDrive}
                </Button>
              </div>

              <div className={styles.collectionSourceFields}>
                <label>
                  <span>{strings.sitesCollectionName}</span>
                  <input
                    className={styles.input}
                    value={draft.name}
                    onChange={(event) => setDraft((current) => ({ ...current, name: event.target.value }))}
                  />
                </label>
                <label>
                  <span>{strings.sitesCollectionBase}</span>
                  <select
                    className={styles.input}
                    value={draft.baseNodeId}
                    onChange={(event) => chooseSource(event.target.value)}
                  >
                    <option value="">{strings.sitesCollectionChooseBase}</option>
                    {sources.map((candidate) => (
                      <option key={candidate.nodeId} value={candidate.nodeId}>{candidate.name}</option>
                    ))}
                  </select>
                </label>
                <label>
                  <span>{strings.sitesCollectionTable}</span>
                  <select
                    className={styles.input}
                    value={draft.baseTableId}
                    onChange={(event) => chooseTable(event.target.value)}
                  >
                    <option value="">{strings.sitesCollectionChooseTable}</option>
                    {(source?.tables ?? []).map((candidate) => (
                      <option key={candidate.id} value={candidate.id}>
                        {candidate.name} · {strings.sitesCollectionRows(candidate.recordCount)}
                      </option>
                    ))}
                  </select>
                </label>
              </div>

              <div className={styles.collectionMappingHead}>
                <Link2 aria-hidden="true" />
                <div>
                  <h3>{strings.sitesCollectionMapping}</h3>
                  <p>{strings.sitesCollectionMappingHint}</p>
                </div>
              </div>
              <div className={styles.collectionMappingGrid}>
                <MappingField
                  label={strings.sitesCollectionTitleField}
                  value={draft.mapping.title}
                  fields={textFields}
                  required
                  onChange={(value) => mapField("title", value)}
                />
                <MappingField label={strings.sitesCollectionSlugField} value={draft.mapping.slug} fields={textFields} onChange={(value) => mapField("slug", value)} />
                <MappingField label={strings.sitesCollectionSummaryField} value={draft.mapping.summary} fields={textFields} onChange={(value) => mapField("summary", value)} />
                <MappingField label={strings.sitesCollectionBodyField} value={draft.mapping.body} fields={textFields} onChange={(value) => mapField("body", value)} />
                <MappingField label={strings.sitesCollectionImageField} value={draft.mapping.image} fields={imageFields} onChange={(value) => mapField("image", value)} />
                <MappingField label={strings.sitesCollectionLinkField} value={draft.mapping.link} fields={textFields} onChange={(value) => mapField("link", value)} />
                <MappingField label={strings.sitesCollectionDateField} value={draft.mapping.publishedAt} fields={dateFields} onChange={(value) => mapField("publishedAt", value)} />
              </div>

              <div className={styles.collectionActions}>
                {selectedId !== null && (
                  <div className={styles.collectionDisconnectGroup}>
                    <Button
                      variant={disconnectArmed ? "danger" : "ghost"}
                      icon={<Unplug size="var(--icon-size-inline)" />}
                      disabled={busy}
                      onClick={() => void disconnect()}
                    >
                      {disconnectArmed
                        ? strings.sitesCollectionDisconnectConfirm
                        : strings.sitesCollectionDisconnect}
                    </Button>
                    {disconnectArmed && <span>{strings.sitesCollectionDisconnectHint}</span>}
                  </div>
                )}
                <Button disabled={busy || !canSave} onClick={() => void save()}>
                  {busy ? strings.sitesCollectionSaving : strings.sitesCollectionSave}
                </Button>
              </div>
            </section>

            <section className={styles.collectionPreview} aria-labelledby="collection-preview-title">
              <div className={styles.collectionPanelHead}>
                <div>
                  <h2 id="collection-preview-title">{strings.sitesCollectionPreview}</h2>
                  <p>{strings.sitesCollectionPreviewHint}</p>
                </div>
              </div>
              {previewError !== null && <ErrorBanner message={previewError} />}
              {previewBusy ? (
                <div className={styles.collectionPreviewSkeleton} role="status" aria-label={strings.sitesCollectionPreviewLoading}>
                  <span />
                  <span />
                  <span />
                </div>
              ) : preview === null ? (
                <div className={styles.collectionPreviewEmpty}>
                  <Rows3 aria-hidden="true" />
                  <strong>{strings.sitesCollectionPreviewSaveTitle}</strong>
                  <span>{strings.sitesCollectionPreviewSaveBody}</span>
                </div>
              ) : preview.items.length === 0 ? (
                <div className={styles.collectionPreviewEmpty}>
                  <Rows3 aria-hidden="true" />
                  <strong>{strings.sitesCollectionPreviewEmptyTitle}</strong>
                  <span>{strings.sitesCollectionPreviewEmptyBody}</span>
                  <Button variant="ghost" onClick={() => navigate("/drive")}>
                    {strings.sitesCollectionOpenDrive}
                  </Button>
                </div>
              ) : (
                <div className={styles.collectionPreviewGrid}>
                  {preview.items.map((item, index) => (
                    <article className={styles.collectionPreviewCard} key={`${item.slug ?? item.title}-${index}`}>
                      {item.imageBlobId !== null && <div className={styles.collectionPreviewImage} aria-hidden="true" />}
                      <div>
                        {item.publishedAt !== null && <time>{item.publishedAt}</time>}
                        <h3>{item.title}</h3>
                        {item.summary !== null && <p>{item.summary}</p>}
                        {item.link !== null && <span>{strings.sitesCollectionPreviewLinked}</span>}
                      </div>
                    </article>
                  ))}
                </div>
              )}
            </section>
          </div>
        </div>
      )}
    </div>
  );
}
