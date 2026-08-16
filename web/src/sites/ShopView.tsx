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
import { ArrowLeft, Package, Plus, Trash2 } from "lucide-react";
import { Link, useNavigate, useParams } from "react-router-dom";

import { Button, Spinner } from "../ds";
import { strings } from "../i18n";
import { sitesMessage, useSitesApi } from "./api";
import { formatPrice, parsePriceInput, priceInput } from "./catalogPricing";
import { DialogFrame, EmptyState, ErrorBanner, Field } from "./parts";
import type { SiteDetail, SiteShopItemList, SiteShopProductList } from "./types";
import styles from "./SitesModule.module.css";

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
      <Field label={strings.sitesShopProduct} hint={strings.sitesShopProductHint}>
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
          : { ...current, items: current.items.filter((row) => row.id !== itemId) },
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
          {site !== null && <span className={styles.submissionSiteName}>{site.name}</span>}
        </div>
        <div className={styles.headerActions}>
          {loading && <Spinner size={16} />}
          {manager && (
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
          )}
        </div>
      </header>

      {error !== null && <ErrorBanner message={error} />}

      {!loading && site !== null && !manager && (
        <p className={styles.hint}>{strings.sitesCommerceReadOnly}</p>
      )}

      {shipping !== null && list !== null && (
        <p className={styles.hint}>
          {shipping === 0
            ? strings.sitesShopDeliveryFree
            : strings.sitesShopDeliveryRate(
                formatPrice(shipping, list.currency, list.currencyExponent),
              )}{" "}
          {manager && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => {
                setDialogError(null);
                setEditingDelivery(true);
              }}
            >
              {strings.sitesShopDeliveryChange}
            </Button>
          )}
        </p>
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
                      <span className={styles.hint}>{strings.sitesShopGoneProduct}</span>
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
                      <span className={styles.hint}>{strings.sitesShopNotStocked}</span>
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

      {armedId !== null && <p className={styles.hint}>{strings.sitesShopRemoveHint}</p>}

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
