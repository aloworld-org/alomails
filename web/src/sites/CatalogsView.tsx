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

const styles = {
  page: "mx-auto flex w-full max-w-7xl flex-col gap-6 px-4 py-6 sm:px-6 lg:px-8",
  header: "flex flex-wrap items-start gap-4 border-b border-subtle pb-5",
  backLink:
    "inline-flex min-h-10 items-center gap-2 rounded-xl px-3 text-sm font-semibold text-secondary no-underline transition hover:bg-muted hover:text-primary",
  title: "text-2xl font-semibold tracking-tight text-primary",
  collectionPageHint: "mt-1 max-w-2xl text-sm leading-6 text-secondary",
  collectionLoading:
    "flex min-h-64 items-center justify-center gap-3 rounded-2xl border border-subtle bg-surface text-sm text-secondary shadow-sm",
  catalogWorkspace:
    "grid min-h-[36rem] gap-5 lg:grid-cols-[18rem_minmax(0,1fr)]",
  catalogList:
    "flex min-w-0 flex-col gap-2 rounded-2xl border border-subtle bg-surface p-3 shadow-sm",
  collectionListEmpty:
    "flex flex-col items-center gap-2 px-4 py-10 text-center text-sm text-secondary [&_svg]:size-8 [&_svg]:text-tertiary [&_strong]:text-primary",
  collectionListItem:
    "flex min-h-16 w-full items-center gap-3 rounded-xl border border-transparent px-3 py-2.5 text-left text-primary transition hover:bg-muted [&>svg]:size-5 [&>svg]:shrink-0 [&>svg]:text-secondary [&_span]:min-w-0 [&_strong]:block [&_strong]:truncate [&_small]:mt-0.5 [&_small]:block [&_small]:truncate [&_small]:text-xs [&_small]:text-secondary",
  collectionListItemActive:
    "!border-accent/20 !bg-accent-soft shadow-sm [&>svg]:!text-accent",
  catalogEditor: "flex min-w-0 flex-col gap-5",
  catalogPanel:
    "flex flex-col gap-5 rounded-2xl border border-subtle bg-surface p-5 shadow-sm sm:p-6",
  collectionPanelHead:
    "flex flex-wrap items-start justify-between gap-4 border-b border-subtle pb-4 [&_h2]:text-lg [&_h2]:font-semibold [&_h2]:text-primary [&_p]:mt-1 [&_p]:text-sm [&_p]:leading-6 [&_p]:text-secondary",
  collectionSourceFields:
    "grid gap-4 sm:grid-cols-[minmax(0,1fr)_10rem] [&_label]:flex [&_label]:flex-col [&_label]:gap-2 [&_label>span]:text-xs [&_label>span]:font-semibold [&_label>span]:uppercase [&_label>span]:tracking-wide [&_label>span]:text-secondary",
  input:
    "min-h-11 w-full rounded-xl border border-default bg-surface px-3.5 py-2.5 text-primary outline-none transition placeholder:text-tertiary focus:border-accent focus:ring-2 focus:ring-accent/15 disabled:bg-muted",
  hint: "text-sm leading-6 text-secondary",
  catalogOrdersToggle:
    "flex cursor-pointer items-start gap-3 rounded-xl bg-muted px-4 py-3.5 [&_input]:mt-0.5 [&_input]:size-5 [&_input]:accent-[var(--accent)] [&_span]:min-w-0 [&_strong]:block [&_strong]:text-sm [&_strong]:text-primary [&_small]:mt-1 [&_small]:block [&_small]:text-sm [&_small]:leading-5 [&_small]:text-secondary",
  collectionActions:
    "flex flex-wrap items-center justify-between gap-3 border-t border-subtle pt-4",
  collectionDisconnectGroup:
    "flex flex-wrap items-center gap-3 text-xs text-danger",
  catalogItemsPanel:
    "flex flex-col gap-5 rounded-2xl border border-subtle bg-surface p-5 shadow-sm sm:p-6",
  catalogGroups:
    "flex flex-col gap-2 rounded-xl border border-subtle bg-muted/40 p-4 [&_h3]:font-semibold [&_h3]:text-primary [&_p]:text-sm [&_p]:text-secondary",
  catalogGroupRows: "mt-2 grid gap-2",
  catalogGroupRow: "grid items-center gap-2 sm:grid-cols-[minmax(0,1fr)_auto]",
  catalogItems:
    "flex list-none flex-col divide-y divide-subtle overflow-hidden rounded-xl border border-subtle p-0",
  catalogItem:
    "grid min-w-0 gap-4 bg-surface p-4 sm:grid-cols-[minmax(0,1fr)_auto] lg:grid-cols-[minmax(0,1fr)_auto_auto] lg:items-center",
  catalogItemText:
    "min-w-0 [&_strong]:block [&_strong]:truncate [&_strong]:text-primary [&_small]:mt-1 [&_small]:block [&_small]:text-xs [&_small]:text-secondary [&_p]:mt-2 [&_p]:line-clamp-2 [&_p]:text-sm [&_p]:leading-5 [&_p]:text-secondary",
  catalogItemPrice:
    "flex flex-wrap items-center gap-2 text-sm font-semibold text-primary",
  chip: "inline-flex min-h-7 items-center rounded-full bg-muted px-2.5 text-xs font-semibold text-secondary",
  catalogItemActions:
    "flex flex-wrap items-center justify-end gap-2 sm:col-span-2 lg:col-span-1",
};

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

  const selected =
    catalogs.find((catalog) => catalog.id === selectedId) ?? null;

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
      const stored =
        creating || selectedId === null
          ? await api.createCatalog(siteId, draft)
          : await api.updateCatalog(siteId, selectedId, draft);
      setCatalogs((current) =>
        current.some((catalog) => catalog.id === stored.id)
          ? current.map((catalog) =>
              catalog.id === stored.id ? stored : catalog,
            )
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
    if (selectedId === null || name.trim() === "" || name === group.name)
      return;
    setError(null);
    try {
      await api.updateCatalogCategory(
        siteId,
        selectedId,
        group.id,
        name.trim(),
      );
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
      detail?.categories.find((group) => group.id === item.categoryId)?.name ??
      null
    );
  }

  function priceOf(item: SiteCatalogItem, catalog: SiteCatalog): string {
    if (item.priceCents === null) return strings.sitesCatalogNoPrice;
    const price = formatPrice(
      item.priceCents,
      catalog.currency,
      catalog.currencyExponent,
    );
    return item.priceNote === null ? price : `${price} ${item.priceNote}`;
  }

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <Link to=".." relative="path" className={styles.backLink}>
          <ArrowLeft size="var(--icon-size-inline)" aria-hidden="true" />
          {strings.sitesBack}
        </Link>
        <div className="min-w-0 flex-1">
          <h1 className={styles.title}>{strings.sitesCatalogs}</h1>
          <p className={styles.collectionPageHint}>
            {strings.sitesCatalogsHint}
          </p>
        </div>
        {!loading && catalogs.length > 0 && (
          <Button
            icon={<Plus size="var(--icon-size-inline)" />}
            onClick={startCreate}
          >
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
          <aside
            className={styles.catalogList}
            aria-label={strings.sitesCatalogs}
          >
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
                  catalog.id === selectedId
                    ? styles.collectionListItemActive
                    : ""
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
            <section
              className={styles.catalogPanel}
              aria-labelledby="catalog-settings-title"
            >
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
                      setDraft((current) => ({
                        ...current,
                        name: event.target.value,
                      }))
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
                      setDraft((current) => ({
                        ...current,
                        currency: event.target.value,
                      }))
                    }
                  />
                  <small className={styles.hint}>
                    {strings.sitesCatalogCurrencyHint}
                  </small>
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
                    {deleteArmed && (
                      <span>{strings.sitesCatalogDeleteHint}</span>
                    )}
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
              <section
                className={styles.catalogItemsPanel}
                aria-labelledby="catalog-items-title"
              >
                <div className={styles.collectionPanelHead}>
                  <div>
                    <h2 id="catalog-items-title">
                      {strings.sitesCatalogItems}
                    </h2>
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
                          onBlur={(event) =>
                            void renameGroup(group, event.target.value)
                          }
                        />
                        <Button
                          variant="ghost"
                          size="sm"
                          icon={<Trash2 size="var(--icon-size-inline)" />}
                          disabled={busy}
                          aria-label={strings.sitesCatalogGroupRemove(
                            group.name,
                          )}
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
                            {item.description !== null && (
                              <p>{item.description}</p>
                            )}
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
                              aria-label={strings.sitesCatalogEditItem(
                                item.name,
                              )}
                              onClick={() => setItemDialog(item)}
                            >
                              {strings.sitesCatalogEdit}
                            </Button>
                            <Button
                              variant={
                                armedItemId === item.id ? "danger" : "ghost"
                              }
                              size="sm"
                              icon={<Trash2 size="var(--icon-size-inline)" />}
                              disabled={busy}
                              aria-label={
                                armedItemId === item.id
                                  ? strings.sitesCatalogItemDeleteConfirmLabel(
                                      item.name,
                                    )
                                  : strings.sitesCatalogItemDeleteLabel(
                                      item.name,
                                    )
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
