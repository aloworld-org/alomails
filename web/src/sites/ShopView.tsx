// The owner's shop window (ADR 0041, S3.05c): which stocked products a site
// lists for sale, over the `/sites/{id}/shop-items` routes, sharing the
// checkout wave one built.
//
// Three facts shape this screen, all inherited from the store rather than
// invented here:
//
//   * **Nothing here is a second copy.** A listing names an item on Billing's
//     price list; the name, the price and the shelf count in every row are
//     the owning seams' answers at this read, and a listing whose product was
//     archived says so instead of showing a price that is no longer anyone's.
//   * **The store rules on what may be listed.** Not on the price list or not
//     stocked is a refusal in the server's own words, shown verbatim.
//   * **Delisting never touches a sale.** Orders keep their own product
//     reference; removing a listing only takes it out of the window.
//
// The one price this screen edits is the site's own: the flat delivery rate
// per order, charged by the public checkout beside the goods.
import { useCallback, useEffect, useMemo, useState } from "react";
import { ArrowLeft, Package, Plus, Sparkles, Trash2 } from "lucide-react";
import { Link, useNavigate, useParams } from "react-router-dom";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import { formatPrice, parsePriceInput, priceInput } from "./catalogPricing";
import { DialogFrame, EmptyState, ErrorBanner, Field } from "./parts";
import type {
  SiteDetail,
  SiteShopItemList,
  SiteShopProductList,
} from "./types";

const styles = {
  page: "mx-auto flex w-full max-w-[90rem] flex-col gap-6 px-5 py-6 sm:px-8 lg:px-10",
  header:
    "flex flex-col gap-4 rounded-2xl border border-subtle bg-surface px-5 py-5 shadow-sm sm:flex-row sm:items-center",
  backLink:
    "inline-flex min-h-10 shrink-0 items-center gap-2 self-start rounded-xl border border-subtle bg-surface px-3.5 text-sm font-semibold text-primary no-underline transition-colors hover:bg-app focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent",
  siteHead: "min-w-0 flex-1",
  title: "m-0 text-2xl font-semibold tracking-tight text-primary",
  submissionSiteName: "mt-1 block truncate text-sm text-secondary",
  headerActions: "flex min-h-10 items-center gap-3 sm:ml-auto",
  hint: "text-sm leading-6 text-secondary",
  input:
    "min-h-11 w-full rounded-xl border border-subtle bg-surface px-3.5 text-base text-primary outline-none transition focus:border-accent focus:ring-2 focus:ring-accent/15",
  tableWrapStatic:
    "overflow-x-auto rounded-2xl border border-subtle bg-surface shadow-sm",
  table:
    "w-full min-w-[42rem] border-collapse text-left [&_th]:border-b [&_th]:border-subtle [&_th]:bg-app [&_th]:px-5 [&_th]:py-3.5 [&_th]:text-xs [&_th]:font-semibold [&_th]:uppercase [&_th]:tracking-wide [&_th]:text-secondary [&_td]:border-b [&_td]:border-subtle [&_td]:px-5 [&_td]:py-4 [&_td]:align-middle [&_td]:text-sm [&_td]:text-primary [&_tbody_tr:last-child_td]:border-b-0 [&_tbody_tr]:transition-colors [&_tbody_tr:hover]:bg-app/70",
} as const;

function AddProductDialog({
  products,
  busy,
  error,
  onClose,
  onAdd,
}: {
  products: SiteShopProductList;
  busy: boolean;
  error: string | null;
  onClose: () => void;
  onAdd: (productId: string) => void;
}) {
  const [productId, setProductId] = useState(products.products[0]?.id ?? "");
  return (
    <DialogFrame
      Icon={Package}
      title={strings.sitesShopAddProduct}
      subtitle={strings.sitesShopAddSubtitle}
      error={error}
      busy={busy}
      canSubmit={productId !== ""}
      submitLabel={strings.sitesShopAddSubmit}
      onClose={onClose}
      onSubmit={() => onAdd(productId)}
    >
      <Field
        label={strings.sitesShopProduct}
        hint={strings.sitesShopProductHint}
      >
        <select
          className={styles.input}
          value={productId}
          onChange={(event) => setProductId(event.target.value)}
          autoFocus
        >
          {products.products.map((product) => (
            <option key={product.id} value={product.id}>
              {strings.sitesShopProductOption(
                product.name,
                formatPrice(
                  product.unitPriceCents,
                  products.currency,
                  products.currencyExponent,
                ),
                product.availableUnits,
              )}
            </option>
          ))}
        </select>
      </Field>
    </DialogFrame>
  );
}

function DeliveryDialog({
  cents,
  currency,
  exponent,
  busy,
  error,
  onClose,
  onSave,
}: {
  cents: number;
  currency: string;
  exponent: number;
  busy: boolean;
  error: string | null;
  onClose: () => void;
  onSave: (cents: number) => void;
}) {
  const [text, setText] = useState(priceInput(cents, exponent));
  const parsed = parsePriceInput(text, exponent);
  return (
    <DialogFrame
      Icon={Package}
      title={strings.sitesShopDeliveryTitle}
      subtitle={strings.sitesShopDeliverySubtitle}
      error={error}
      busy={busy}
      canSubmit={parsed !== null}
      submitLabel={strings.sitesShopDeliverySave}
      onClose={onClose}
      onSubmit={() => {
        if (parsed !== null) onSave(parsed);
      }}
    >
      <Field
        label={strings.sitesShopDeliveryLabel(currency)}
        hint={strings.sitesShopDeliveryHint}
      >
        <input
          className={styles.input}
          inputMode="decimal"
          value={text}
          onChange={(event) => setText(event.target.value)}
          autoFocus
        />
      </Field>
    </DialogFrame>
  );
}

export function ShopView() {
  const { siteId = "" } = useParams();
  const api = useSitesApi();
  const navigate = useNavigate();
  const [site, setSite] = useState<SiteDetail | null>(null);
  const [list, setList] = useState<SiteShopItemList | null>(null);
  const [products, setProducts] = useState<SiteShopProductList | null>(null);
  const [shipping, setShipping] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [adding, setAdding] = useState(false);
  const [editingDelivery, setEditingDelivery] = useState(false);
  const [dialogBusy, setDialogBusy] = useState(false);
  const [dialogError, setDialogError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [armedId, setArmedId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      // The detail first: whether the caller manages this site decides
      // whether the picker — a read of the whole price list the server
      // refuses a collaborator (S3.06a) — is asked for at all.
      const detail = await api.site(siteId);
      const [shelf, rate, candidates] = await Promise.all([
        api.shopItems(siteId),
        api.shopShipping(siteId),
        detail.canManageCollaborators
          ? api.shopProducts(siteId)
          : Promise.resolve(null),
      ]);
      setSite(detail);
      setList(shelf);
      setProducts(candidates);
      setShipping(rate);
      setError(null);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesShopLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, siteId]);

  useEffect(() => {
    void load();
  }, [load]);

  const items = useMemo(() => list?.items ?? [], [list]);

  /** What the add dialog offers: the stocked products not already listed —
   *  offering a row the store would refuse as a duplicate helps nobody. */
  const addable = useMemo(() => {
    if (products === null) return null;
    const listed = new Set(items.map((item) => item.productId));
    return {
      ...products,
      products: products.products.filter((product) => !listed.has(product.id)),
    };
  }, [products, items]);

  async function add(productId: string) {
    setDialogBusy(true);
    setDialogError(null);
    try {
      await api.addShopItem(siteId, productId);
      setAdding(false);
      await load();
    } catch (reason) {
      setDialogError(sitesMessage(reason, strings.sitesShopAddFailed));
    } finally {
      setDialogBusy(false);
    }
  }

  async function saveDelivery(cents: number) {
    setDialogBusy(true);
    setDialogError(null);
    try {
      await api.setShopShipping(siteId, cents);
      setShipping(cents);
      setEditingDelivery(false);
    } catch (reason) {
      setDialogError(sitesMessage(reason, strings.sitesShopDeliveryFailed));
    } finally {
      setDialogBusy(false);
    }
  }

  async function remove(itemId: string) {
    if (armedId !== itemId) {
      setArmedId(itemId);
      return;
    }
    setBusyId(itemId);
    setError(null);
    try {
      await api.removeShopItem(siteId, itemId);
      setList((current) =>
        current === null
          ? current
          : {
              ...current,
              items: current.items.filter((row) => row.id !== itemId),
            },
      );
      setArmedId(null);
    } catch (reason) {
      setError(sitesMessage(reason, strings.sitesShopRemoveFailed));
    } finally {
      setBusyId(null);
    }
  }

  const manager = site !== null && site.canManageCollaborators;
  const noCandidates = products !== null && products.products.length === 0;
  const allListed =
    addable !== null && addable.products.length === 0 && !noCandidates;

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <Link to=".." relative="path" className={styles.backLink}>
          <ArrowLeft size="var(--icon-size-inline)" aria-hidden="true" />
          {strings.sitesBack}
        </Link>
        <div className={styles.siteHead}>
          <h1 className={styles.title}>{strings.sitesShop}</h1>
          {site !== null && (
            <span className={styles.submissionSiteName}>{site.name}</span>
          )}
        </div>
        <div className={styles.headerActions}>
          {loading && <Spinner size={16} />}
          {manager && (
            <>
              {products !== null && !noCandidates && (
                <Button
                  variant="secondary"
                  size="sm"
                  icon={<Sparkles size="var(--icon-size-inline)" />}
                  onClick={() => navigate(`/sites/${siteId}/shop-setup`)}
                >
                  {strings.sitesShopSetup}
                </Button>
              )}
              <Button
                size="sm"
                icon={<Plus size="var(--icon-size-inline)" />}
                disabled={addable === null || addable.products.length === 0}
                onClick={() => {
                  setDialogError(null);
                  setAdding(true);
                }}
              >
                {strings.sitesShopAddProduct}
              </Button>
            </>
          )}
        </div>
      </header>

      {error !== null && <ErrorBanner message={error} />}

      {loading && (
        <div
          className="flex min-h-64 items-center justify-center rounded-2xl border border-subtle bg-surface shadow-sm"
          role="status"
          aria-label={strings.sitesShop}
        >
          <Spinner size={22} />
        </div>
      )}

      {/* A status, not a paragraph: the read-only fact arrives after the
          load, and a screen reader that has already moved past the header
          would otherwise never hear it (S3.06a). */}
      {!loading && site !== null && !manager && (
        <p className={styles.hint} role="status">
          {strings.sitesCommerceReadOnly}
        </p>
      )}

      {shipping !== null && list !== null && (
        <section className="flex flex-col gap-3 rounded-2xl border border-subtle bg-surface px-5 py-4 shadow-sm sm:flex-row sm:items-center">
          <div className="flex min-w-0 flex-1 items-center gap-3">
            <span className="flex size-10 shrink-0 items-center justify-center rounded-xl bg-accent-soft text-accent">
              <Package size={19} aria-hidden="true" />
            </span>
            <p className="m-0 text-sm font-medium text-primary">
              {shipping === 0
                ? strings.sitesShopDeliveryFree
                : strings.sitesShopDeliveryRate(
                    formatPrice(shipping, list.currency, list.currencyExponent),
                  )}
            </p>
          </div>
          {manager && (
            <Button
              variant="secondary"
              size="sm"
              onClick={() => {
                setDialogError(null);
                setEditingDelivery(true);
              }}
            >
              {strings.sitesShopDeliveryChange}
            </Button>
          )}
        </section>
      )}

      {!loading && manager && noCandidates && items.length === 0 && (
        <EmptyState
          Icon={Package}
          title={strings.sitesShopNoProducts}
          body={strings.sitesShopNoProductsHint}
          cta={strings.sitesShopSetup}
          onCta={() => navigate(`/sites/${siteId}/shop-setup`)}
        />
      )}

      {!loading && manager && !noCandidates && items.length === 0 && (
        <EmptyState
          Icon={Package}
          title={strings.sitesShopEmptyTitle}
          body={strings.sitesShopEmptyBody}
          cta={strings.sitesShopAddProduct}
          onCta={() => {
            setDialogError(null);
            setAdding(true);
          }}
        />
      )}

      {!loading && site !== null && !manager && items.length === 0 && (
        <EmptyState
          Icon={Package}
          title={strings.sitesShopEmptyTitle}
          body={strings.sitesCommerceReadOnly}
        />
      )}

      {allListed && <p className={styles.hint}>{strings.sitesShopAllListed}</p>}

      {items.length > 0 && list !== null && (
        <div className={styles.tableWrapStatic}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th scope="col">{strings.sitesShopColWhat}</th>
                <th scope="col">{strings.sitesShopColPrice}</th>
                <th scope="col">{strings.sitesShopColShelf}</th>
                {manager && <th scope="col">{strings.sitesColActions}</th>}
              </tr>
            </thead>
            <tbody>
              {items.map((item) => (
                <tr key={item.id}>
                  <td>
                    {item.productName ?? (
                      <span className={styles.hint}>
                        {strings.sitesShopGoneProduct}
                      </span>
                    )}
                  </td>
                  <td>
                    {item.unitPriceCents === null
                      ? "—"
                      : formatPrice(
                          item.unitPriceCents,
                          list.currency,
                          list.currencyExponent,
                        )}
                  </td>
                  <td>
                    {item.availableUnits === null ? (
                      <span className={styles.hint}>
                        {strings.sitesShopNotStocked}
                      </span>
                    ) : (
                      strings.sitesShopUnits(item.availableUnits)
                    )}
                  </td>
                  {manager && (
                    <td>
                      <Button
                        variant={armedId === item.id ? "danger" : "ghost"}
                        size="sm"
                        icon={<Trash2 size="var(--icon-size-inline)" />}
                        // Named per product (S2.16b2): a shelf of rows is
                        // otherwise a column of identical "Remove" buttons.
                        aria-label={
                          armedId === item.id
                            ? strings.sitesShopRemoveConfirm
                            : strings.sitesShopRemoveFor(
                                item.productName ??
                                  strings.sitesShopGoneProduct,
                              )
                        }
                        disabled={busyId === item.id}
                        onClick={() => void remove(item.id)}
                      >
                        {armedId === item.id
                          ? strings.sitesShopRemoveConfirm
                          : strings.sitesShopRemove}
                      </Button>
                    </td>
                  )}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* The arming is a renamed button, which nothing announces; this
          sentence appearing in a live region is what says it out loud. */}
      {armedId !== null && (
        <p className={styles.hint} role="status">
          {strings.sitesShopRemoveHint}
        </p>
      )}

      {adding && addable !== null && addable.products.length > 0 && (
        <AddProductDialog
          products={addable}
          busy={dialogBusy}
          error={dialogError}
          onClose={() => setAdding(false)}
          onAdd={(productId) => void add(productId)}
        />
      )}

      {editingDelivery && shipping !== null && list !== null && (
        <DeliveryDialog
          cents={shipping}
          currency={list.currency}
          exponent={list.currencyExponent}
          busy={dialogBusy}
          error={dialogError}
          onClose={() => setEditingDelivery(false)}
          onSave={(cents) => void saveDelivery(cents)}
        />
      )}
    </div>
  );
}
