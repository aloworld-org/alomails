// What this website offers, in one screen: the catalogs of a site, the groups
// inside one, and the items themselves (ADR 0036 / ADR 0041, S2.12c).
//
// The shape follows the CMS screen next door — the list of things on the left,
// the one being worked on beside it — because a person who has connected a
// collection already knows how this works. Two facts the screen has to keep
// saying out loud, because both surprise people otherwise: a price is frozen
// at publish time, so an edit here changes nothing live until the site is
// published again; and taking orders is a switch on the catalog, not on the
// page that shows it.
//
// Nothing is validated here. Handles, prices, currencies and lengths are ruled
// on by the store, and its refusal — a sentence naming the rule — is what the
// screen shows.
import { useCallback, useEffect, useState } from "react";
import {
  ArrowLeft,
  Pencil,
  Plus,
  ShoppingBag,
  Tag,
  Trash2,
} from "lucide-react";
import { Link, useParams } from "react-router-dom";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import { availabilityLabel, CatalogItemDialog } from "./CatalogItemDialog";
import { formatPrice } from "./catalogPricing";
import { EmptyState, ErrorBanner } from "./parts";
import type {
  SiteCatalog,
  SiteCatalogCategory,
  SiteCatalogDetail,
  SiteCatalogDraft,
  SiteCatalogItem,
} from "./types";
import styles from "./SitesModule.module.css";

/** A new catalog starts in euros: the product is sold in the eurozone, and a
 *  blank currency field is a question nobody wants to answer twice. */
const emptyDraft = (): SiteCatalogDraft => ({
  name: "",
  currency: "EUR",
  ordersEnabled: false,
});

function draftFrom(catalog: SiteCatalog): SiteCatalogDraft {
  return {
    name: catalog.name,
    currency: catalog.currency,
    ordersEnabled: catalog.ordersEnabled,
  };
}

/** Which item, if any, the dialog is open on. `"new"` is an item being added;
 *  an item value is one being changed. */
type ItemDialog = "new" | SiteCatalogItem | null;

export function CatalogsView() {
  const { siteId = "" } = useParams();
  const api = useSitesApi();
  const [catalogs, setCatalogs] = useState<SiteCatalog[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<SiteCatalogDetail | null>(null);
  const [draft, setDraft] = useState<SiteCatalogDraft>(emptyDraft);
  const [creating, setCreating] = useState(false);
  const [newGroup, setNewGroup] = useState("");
  const [itemDialog, setItemDialog] = useState<ItemDialog>(null);
  const [loading, setLoading] = useState(true);
  const [detailBusy, setDetailBusy] = useState(false);
  const [busy, setBusy] = useState(false);
  const [deleteArmed, setDeleteArmed] = useState(false);
  const [armedItemId, setArmedItemId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const stored = await api.catalogs(siteId);
      setCatalogs(stored);
      const first = stored[0];
      setSelectedId(first?.id ?? null);
      setDraft(first === undefined ? emptyDraft() : draftFrom(first));
      setCreating(first === undefined);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesCatalogsLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, siteId]);

  useEffect(() => {
    void load();
  }, [load]);

  // The selected catalog's own contents. A catalog that will not load costs
  // the item list, not the screen: the settings panel above it still works.
  useEffect(() => {
    setDeleteArmed(false);
    setArmedItemId(null);
    setNewGroup("");
    if (selectedId === null) {
      setDetail(null);
      return;
    }
    let cancelled = false;
    setDetailBusy(true);
    void api
      .catalog(siteId, selectedId)
      .then(
        (stored) => {
          if (!cancelled) setDetail(stored);
        },
        (reason: unknown) => {
          if (!cancelled) {
            setDetail(null);
            setError(sitesMessage(reason, strings.sitesCatalogLoadFailed));
          }
        },
      )
      .finally(() => {
        if (!cancelled) setDetailBusy(false);
      });
    return () => {
      cancelled = true;
    };
  }, [api, selectedId, siteId]);

  const selected = catalogs.find((catalog) => catalog.id === selectedId) ?? null;

  function select(catalog: SiteCatalog) {
    setSelectedId(catalog.id);
    setDraft(draftFrom(catalog));
    setCreating(false);
    setError(null);
  }

  function startCreate() {
    setSelectedId(null);
    setDetail(null);
    setDraft(emptyDraft());
    setCreating(true);
    setError(null);
  }

  /** Re-reads the selected catalog after a write that changed its contents.
   *  One read of the server's own answer, rather than a second copy of the
   *  ordering and grouping rules maintained here. */
  async function refreshDetail(catalogId: string) {
    try {
      setDetail(await api.catalog(siteId, catalogId));
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesCatalogLoadFailed));
    }
  }

  async function saveCatalog() {
    if (draft.name.trim() === "") return;
    setBusy(true);
    setError(null);
    try {
      const stored = creating || selectedId === null
        ? await api.createCatalog(siteId, draft)
        : await api.updateCatalog(siteId, selectedId, draft);
      setCatalogs((current) =>
        current.some((catalog) => catalog.id === stored.id)
          ? current.map((catalog) => (catalog.id === stored.id ? stored : catalog))
          : [...current, stored],
      );
      setSelectedId(stored.id);
      setDraft(draftFrom(stored));
      setCreating(false);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesCatalogSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  async function removeCatalog() {
    if (selectedId === null) return;
    if (!deleteArmed) {
      setDeleteArmed(true);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await api.deleteCatalog(siteId, selectedId);
      const remaining = catalogs.filter((catalog) => catalog.id !== selectedId);
      setCatalogs(remaining);
      const next = remaining[0];
      setSelectedId(next?.id ?? null);
      setDraft(next === undefined ? emptyDraft() : draftFrom(next));
      setCreating(next === undefined);
      setDeleteArmed(false);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesCatalogDeleteFailed));
    } finally {
      setBusy(false);
    }
  }

  async function addGroup() {
    if (selectedId === null || newGroup.trim() === "") return;
    setBusy(true);
    setError(null);
    try {
      await api.createCatalogCategory(siteId, selectedId, newGroup.trim());
      setNewGroup("");
      await refreshDetail(selectedId);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesCatalogGroupSaveFailed));
    } finally {
      setBusy(false);
    }
  }

  async function renameGroup(group: SiteCatalogCategory, name: string) {
    if (selectedId === null || name.trim() === "" || name === group.name) return;
    setError(null);
    try {
      await api.updateCatalogCategory(siteId, selectedId, group.id, name.trim());
      await refreshDetail(selectedId);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesCatalogGroupSaveFailed));
    }
  }

  async function removeGroup(group: SiteCatalogCategory) {
    if (selectedId === null) return;
    setBusy(true);
    setError(null);
    try {
      await api.deleteCatalogCategory(siteId, selectedId, group.id);
      await refreshDetail(selectedId);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesCatalogGroupDeleteFailed));
    } finally {
      setBusy(false);
    }
  }

  async function removeItem(item: SiteCatalogItem) {
    if (selectedId === null) return;
    if (armedItemId !== item.id) {
      setArmedItemId(item.id);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await api.deleteCatalogItem(siteId, selectedId, item.id);
      setArmedItemId(null);
      await refreshDetail(selectedId);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesCatalogItemDeleteFailed));
    } finally {
      setBusy(false);
    }
  }

  function groupName(item: SiteCatalogItem): string | null {
    if (item.categoryId === null) return null;
    return (
      detail?.categories.find((group) => group.id === item.categoryId)?.name ?? null
    );
  }

  function priceOf(item: SiteCatalogItem, catalog: SiteCatalog): string {
    if (item.priceCents === null) return strings.sitesCatalogNoPrice;
    const price = formatPrice(item.priceCents, catalog.currency, catalog.currencyExponent);
    return item.priceNote === null ? price : `${price} ${item.priceNote}`;
  }

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <Link to=".." relative="path" className={styles.backLink}>
          <ArrowLeft size="var(--icon-size-inline)" aria-hidden="true" />
          {strings.sitesBack}
        </Link>
        <div>
          <h1 className={styles.title}>{strings.sitesCatalogs}</h1>
          <p className={styles.collectionPageHint}>{strings.sitesCatalogsHint}</p>
        </div>
        {!loading && catalogs.length > 0 && (
          <Button icon={<Plus size="var(--icon-size-inline)" />} onClick={startCreate}>
            {strings.sitesNewCatalog}
          </Button>
        )}
      </header>

      {error !== null && <ErrorBanner message={error} />}

      {loading ? (
        <div className={styles.collectionLoading} role="status">
          <Spinner size={20} />
          <span>{strings.sitesCatalogsLoading}</span>
        </div>
      ) : catalogs.length === 0 && !creating ? (
        <EmptyState
          Icon={ShoppingBag}
          title={strings.sitesCatalogNoneTitle}
          body={strings.sitesCatalogNoneBody}
          cta={strings.sitesNewCatalog}
          onCta={startCreate}
        />
      ) : (
        <div className={styles.catalogWorkspace}>
          <aside className={styles.catalogList} aria-label={strings.sitesCatalogs}>
            {catalogs.length === 0 && (
              <div className={styles.collectionListEmpty}>
                <ShoppingBag aria-hidden="true" />
                <strong>{strings.sitesCatalogNoneTitle}</strong>
                <span>{strings.sitesCatalogNoneBody}</span>
              </div>
            )}
            {catalogs.map((catalog) => (
              <button
                key={catalog.id}
                type="button"
                className={`${styles.collectionListItem} ${
                  catalog.id === selectedId ? styles.collectionListItemActive : ""
                }`}
                aria-pressed={catalog.id === selectedId}
                onClick={() => select(catalog)}
              >
                <ShoppingBag aria-hidden="true" />
                <span>
                  <strong>{catalog.name}</strong>
                  <small>
                    {catalog.currency} ·{" "}
                    {catalog.ordersEnabled
                      ? strings.sitesCatalogOrdersOn
                      : strings.sitesCatalogOrdersOff}
                  </small>
                </span>
              </button>
            ))}
          </aside>

          <div className={styles.catalogEditor}>
            <section className={styles.catalogPanel} aria-labelledby="catalog-settings-title">
              <div className={styles.collectionPanelHead}>
                <div>
                  <h2 id="catalog-settings-title">
                    {creating || selected === null
                      ? strings.sitesNewCatalog
                      : strings.sitesCatalogSettings}
                  </h2>
                  <p>{strings.sitesCatalogSettingsHint}</p>
                </div>
              </div>

              <div className={styles.collectionSourceFields}>
                <label>
                  <span>{strings.sitesCatalogName}</span>
                  <input
                    className={styles.input}
                    value={draft.name}
                    onChange={(event) =>
                      setDraft((current) => ({ ...current, name: event.target.value }))
                    }
                  />
                </label>
                <label>
                  <span>{strings.sitesCatalogCurrency}</span>
                  <input
                    className={styles.input}
                    value={draft.currency}
                    maxLength={3}
                    autoCapitalize="characters"
                    autoCorrect="off"
                    spellCheck={false}
                    onChange={(event) =>
                      setDraft((current) => ({ ...current, currency: event.target.value }))
                    }
                  />
                  <small className={styles.hint}>{strings.sitesCatalogCurrencyHint}</small>
                </label>
              </div>

              <label className={styles.catalogOrdersToggle}>
                <input
                  type="checkbox"
                  checked={draft.ordersEnabled}
                  onChange={(event) =>
                    setDraft((current) => ({
                      ...current,
                      ordersEnabled: event.target.checked,
                    }))
                  }
                />
                <span>
                  <strong>{strings.sitesCatalogOrders}</strong>
                  <small>{strings.sitesCatalogOrdersHint}</small>
                </span>
              </label>

              <div className={styles.collectionActions}>
                {!creating && selected !== null && (
                  <div className={styles.collectionDisconnectGroup}>
                    <Button
                      variant={deleteArmed ? "danger" : "ghost"}
                      icon={<Trash2 size="var(--icon-size-inline)" />}
                      disabled={busy}
                      onClick={() => void removeCatalog()}
                    >
                      {deleteArmed
                        ? strings.sitesCatalogDeleteConfirm
                        : strings.sitesCatalogDelete}
                    </Button>
                    {deleteArmed && <span>{strings.sitesCatalogDeleteHint}</span>}
                  </div>
                )}
                <Button
                  disabled={busy || draft.name.trim() === ""}
                  onClick={() => void saveCatalog()}
                >
                  {creating || selected === null
                    ? strings.sitesCatalogCreate
                    : strings.sitesCatalogSave}
                </Button>
              </div>
            </section>

            {!creating && selected !== null && (
              <section className={styles.catalogItemsPanel} aria-labelledby="catalog-items-title">
                <div className={styles.collectionPanelHead}>
                  <div>
                    <h2 id="catalog-items-title">{strings.sitesCatalogItems}</h2>
                    <p>{strings.sitesCatalogItemsHint}</p>
                  </div>
                  <Button
                    icon={<Plus size="var(--icon-size-inline)" />}
                    onClick={() => setItemDialog("new")}
                  >
                    {strings.sitesCatalogAddItem}
                  </Button>
                </div>

                <div className={styles.catalogGroups}>
                  <h3>{strings.sitesCatalogGroups}</h3>
                  <p>{strings.sitesCatalogGroupsHint}</p>
                  <div className={styles.catalogGroupRows}>
                    {(detail?.categories ?? []).map((group) => (
                      <div className={styles.catalogGroupRow} key={group.id}>
                        <input
                          className={styles.input}
                          defaultValue={group.name}
                          aria-label={strings.sitesCatalogGroupName}
                          onBlur={(event) => void renameGroup(group, event.target.value)}
                        />
                        <Button
                          variant="ghost"
                          size="sm"
                          icon={<Trash2 size="var(--icon-size-inline)" />}
                          disabled={busy}
                          aria-label={strings.sitesCatalogGroupRemove(group.name)}
                          onClick={() => void removeGroup(group)}
                        >
                          {strings.sitesCatalogGroupRemoveShort}
                        </Button>
                      </div>
                    ))}
                    <div className={styles.catalogGroupRow}>
                      <input
                        className={styles.input}
                        value={newGroup}
                        placeholder={strings.sitesCatalogNewGroupPlaceholder}
                        aria-label={strings.sitesCatalogNewGroup}
                        onChange={(event) => setNewGroup(event.target.value)}
                        onKeyDown={(event) => {
                          if (event.key === "Enter") {
                            event.preventDefault();
                            void addGroup();
                          }
                        }}
                      />
                      <Button
                        variant="ghost"
                        size="sm"
                        disabled={busy || newGroup.trim() === ""}
                        onClick={() => void addGroup()}
                      >
                        {strings.sitesCatalogAddGroup}
                      </Button>
                    </div>
                  </div>
                </div>

                {detailBusy && detail === null ? (
                  <div className={styles.collectionLoading} role="status">
                    <Spinner size={20} />
                    <span>{strings.sitesCatalogsLoading}</span>
                  </div>
                ) : (detail?.items.length ?? 0) === 0 ? (
                  <EmptyState
                    Icon={Tag}
                    title={strings.sitesCatalogNoItemsTitle}
                    body={strings.sitesCatalogNoItemsBody}
                    cta={strings.sitesCatalogAddItem}
                    onCta={() => setItemDialog("new")}
                  />
                ) : (
                  <ul className={styles.catalogItems}>
                    {(detail?.items ?? []).map((item) => {
                      const group = groupName(item);
                      return (
                        <li className={styles.catalogItem} key={item.id}>
                          <div className={styles.catalogItemText}>
                            <strong>{item.name}</strong>
                            <small>
                              {item.slug}
                              {group !== null && ` · ${group}`}
                            </small>
                            {item.description !== null && <p>{item.description}</p>}
                          </div>
                          <div className={styles.catalogItemPrice}>
                            <span>{priceOf(item, selected)}</span>
                            {item.availability !== "available" && (
                              <span className={styles.chip}>
                                {availabilityLabel(item.availability)}
                              </span>
                            )}
                          </div>
                          <div className={styles.catalogItemActions}>
                            <Button
                              variant="ghost"
                              size="sm"
                              icon={<Pencil size="var(--icon-size-inline)" />}
                              aria-label={strings.sitesCatalogEditItem(item.name)}
                              onClick={() => setItemDialog(item)}
                            >
                              {strings.sitesCatalogEdit}
                            </Button>
                            <Button
                              variant={armedItemId === item.id ? "danger" : "ghost"}
                              size="sm"
                              icon={<Trash2 size="var(--icon-size-inline)" />}
                              disabled={busy}
                              aria-label={
                                armedItemId === item.id
                                  ? strings.sitesCatalogItemDeleteConfirmLabel(item.name)
                                  : strings.sitesCatalogItemDeleteLabel(item.name)
                              }
                              onClick={() => void removeItem(item)}
                            >
                              {armedItemId === item.id
                                ? strings.sitesCatalogItemDeleteConfirm
                                : strings.sitesCatalogItemDelete}
                            </Button>
                          </div>
                        </li>
                      );
                    })}
                  </ul>
                )}
              </section>
            )}
          </div>
        </div>
      )}

      {itemDialog !== null && selected !== null && (
        <CatalogItemDialog
          siteId={siteId}
          catalog={selected}
          categories={detail?.categories ?? []}
          item={itemDialog === "new" ? null : itemDialog}
          onClose={() => setItemDialog(null)}
          onSaved={() => {
            setItemDialog(null);
            void refreshDetail(selected.id);
          }}
        />
      )}
    </div>
  );
}
