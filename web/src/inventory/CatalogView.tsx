// The catalog: the same rows Billing's price list shows, seen as **things**
// rather than as prices (B5.09a).
//
// The columns are what a warehouse knows an item by — its code, the code on the
// box, whether it has a quantity at all, what it costs us, and how much of it
// there is — and the price columns are on the right, where a person who came
// here to find a product does not have to read past them.
//
// **The editor is Billing's product dialog**, with the supplier choices this
// screen has loaded and Billing's own price list has not. A product is one row
// (`docs/design/inventory.md` § The catalog): a second product form would drift
// from the first about what a product is, and every line ever raised from one
// would inherit the drift.
//
// **On-hand is the ledger's, added up across places.** The one sum this screen
// makes is over integer milli-units of the same product — a count of things,
// exact in integers, and never money: the reference value belongs to the stock
// screen, which shows it beside the place it is in and labels it for what it is.
//
// A **service** shows no quantity at all, and that is not a blank cell to be
// filled in later: the move ledger refuses to move a service, so a number there
// would describe something that cannot exist.
import { useCallback, useEffect, useMemo, useState } from "react";
import { Package, Plus } from "lucide-react";

import {
  ProductDialog,
  billingMessage,
  formatAmount,
  formatRate,
  useBillingApi,
  type BillingProduct,
} from "../billing";
import { Button, Spinner, useDialogs } from "../ds";
import { getLocale, strings } from "../i18n";
import { inventoryMessage, useInventoryApi } from "./api";
import { qtyLabel } from "./format";
import { EmptyState, ErrorBanner } from "./parts";
import type { InvSupplier } from "./types";
import styles from "./InventoryModule.module.css";

/** What is being edited: an existing product, or the new one being added. */
type Editing = { product: BillingProduct | null };

export function CatalogView() {
  const billing = useBillingApi();
  const inventory = useInventoryApi();
  const { confirm } = useDialogs();
  const locale = getLocale();
  const [products, setProducts] = useState<BillingProduct[]>([]);
  /** Milli-units on hand per product, across every real place, as the server's
   *  own rows add up. Absent means the ledger has no row for it — which for a
   *  stocked product is an honest zero. */
  const [onHand, setOnHand] = useState<Map<string, number>>(new Map());
  const [suppliers, setSuppliers] = useState<InvSupplier[]>([]);
  const [search, setSearch] = useState("");
  const [includeArchived, setIncludeArchived] = useState(false);
  const [stockedOnly, setStockedOnly] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [editing, setEditing] = useState<Editing | null>(null);
  const [revision, setRevision] = useState(0);

  const reload = useCallback(() => setRevision((r) => r + 1), []);

  useEffect(() => {
    let live = true;
    setLoading(true);
    void (async () => {
      try {
        // The record and the quantity are two reads of two surfaces, asked for
        // together: a catalog row that showed a name without its stock would be
        // a screen a person has to cross-check by hand.
        const [catalog, stock] = await Promise.all([
          billing.products(includeArchived),
          inventory.stock(),
        ]);
        if (!live) return;
        const totals = new Map<string, number>();
        for (const level of stock.stock) {
          // Real places only, which is what the default read returns anyway;
          // stated here so that adding `includeVirtual` to this call later
          // cannot silently turn a counterparty into stock on a shelf.
          if (!level.real) continue;
          totals.set(level.productId, (totals.get(level.productId) ?? 0) + level.qtyMilli);
        }
        setProducts(catalog);
        setOnHand(totals);
        setError(null);
      } catch (err) {
        // Either surface can be the one that refused, and both fail through the
        // same `Problem` shape — so whichever sentence the server sent is the
        // one shown, verbatim.
        if (live) setError(inventoryMessage(err, strings.inventoryLoadFailed));
      } finally {
        if (live) setLoading(false);
      }
    })();
    return () => {
      live = false;
    };
  }, [billing, inventory, includeArchived, revision]);

  // The pickers' own read, kept apart from the list's: a failure to load
  // suppliers must not blank the catalog, and the dialog simply offers no
  // choices when it happens.
  useEffect(() => {
    let live = true;
    void inventory
      .suppliers()
      .then((list) => {
        if (live) setSuppliers(list.filter((supplier) => !supplier.archived));
      })
      .catch(() => {
        // Nothing to say on the list screen: the picker is a field in a dialog
        // that is not open, and an empty picker is not a broken catalog.
      });
    return () => {
      live = false;
    };
  }, [inventory]);

  const shown = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return products.filter((product) => {
      if (stockedOnly && !product.stocked) return false;
      if (needle === "") return true;
      return `${product.name} ${product.sku} ${product.barcode} ${product.unit}`
        .toLowerCase()
        .includes(needle);
    });
  }, [products, search, stockedOnly]);

  async function toggleArchived(product: BillingProduct) {
    if (
      !product.archived &&
      !(await confirm({
        title: strings.inventoryArchive,
        message: strings.inventoryArchiveProductConfirm(product.name),
        confirmLabel: strings.inventoryArchive,
      }))
    ) {
      return;
    }
    try {
      await billing.setProductArchived(product.id, !product.archived);
      reload();
    } catch (err) {
      setError(billingMessage(err, strings.inventorySaveFailed));
    }
  }

  return (
    <div className={styles.page}>
      <div className={styles.toolbar}>
        <input
          className={styles.search}
          type="search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={strings.inventorySearchCatalog}
          aria-label={strings.inventorySearchCatalog}
        />
        <label className={styles.toggle}>
          <input
            type="checkbox"
            checked={stockedOnly}
            onChange={(e) => setStockedOnly(e.target.checked)}
          />
          {strings.inventoryStockedOnly}
        </label>
        <label className={styles.toggle}>
          <input
            type="checkbox"
            checked={includeArchived}
            onChange={(e) => setIncludeArchived(e.target.checked)}
          />
          {strings.inventoryShowArchived}
        </label>
        <span className={styles.toolbarSpacer} />
        {loading && <Spinner size={16} />}
        <Button onClick={() => setEditing({ product: null })}>
          <Plus size={16} /> {strings.inventoryNewProduct}
        </Button>
      </div>

      {error !== null && <ErrorBanner message={error} />}

      {products.length === 0 && !loading ? (
        <EmptyState
          Icon={Package}
          title={strings.inventoryCatalogEmptyTitle}
          body={strings.inventoryCatalogEmptyBody}
          cta={strings.inventoryNewProduct}
          onCta={() => setEditing({ product: null })}
        />
      ) : shown.length === 0 && !loading ? (
        <p className={styles.noMatches}>{strings.inventoryNoMatches}</p>
      ) : (
        <div className={styles.tableWrap}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th scope="col">{strings.inventoryColProduct}</th>
                <th scope="col">{strings.inventoryColSku}</th>
                <th scope="col">{strings.inventoryColBarcode}</th>
                <th scope="col" className={styles.numeric}>
                  {strings.inventoryColOnHand}
                </th>
                <th scope="col" className={styles.numeric}>
                  {strings.inventoryColPurchasePrice}
                </th>
                <th scope="col" className={styles.numeric}>
                  {strings.inventoryColSalePrice}
                </th>
                <th scope="col" className={styles.numeric}>
                  {strings.inventoryColVatRate}
                </th>
                <th scope="col">
                  <span className={styles.srOnly}>{strings.inventoryColActions}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {shown.map((product) => (
                <tr key={product.id} className={product.archived ? styles.archivedRow : undefined}>
                  <td>
                    <button
                      type="button"
                      className={styles.rowName}
                      onClick={() => setEditing({ product })}
                    >
                      {product.name}
                    </button>
                    <span className={styles.subtle}>
                      {product.stocked
                        ? strings.inventoryTypeStocked
                        : strings.inventoryTypeService}
                      {product.unit !== "" && ` · ${product.unit}`}
                      {product.archived && ` · ${strings.inventoryArchived}`}
                    </span>
                  </td>
                  <td className={styles.code}>{product.sku}</td>
                  <td className={styles.code}>{product.barcode}</td>
                  <td className={styles.numeric}>
                    {product.stocked ? (
                      qtyLabel(onHand.get(product.id) ?? 0)
                    ) : (
                      <span className={styles.muted}>{strings.inventoryNotStocked}</span>
                    )}
                  </td>
                  <td className={styles.numeric}>
                    {formatAmount(product.purchasePriceCents, locale)}
                  </td>
                  <td className={styles.numeric}>{formatAmount(product.unitPriceCents, locale)}</td>
                  <td className={styles.numeric}>{formatRate(product.vatRateBp, locale)}</td>
                  <td className={styles.rowActions}>
                    <button
                      type="button"
                      className={styles.linkAction}
                      onClick={() => setEditing({ product })}
                    >
                      {strings.inventoryEdit}
                    </button>
                    <button
                      type="button"
                      className={styles.linkAction}
                      onClick={() => void toggleArchived(product)}
                    >
                      {product.archived ? strings.inventoryRestore : strings.inventoryArchive}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {editing !== null && (
        <ProductDialog
          product={editing.product}
          suppliers={suppliers.map((supplier) => ({ id: supplier.id, name: supplier.name }))}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            reload();
          }}
        />
      )}
    </div>
  );
}
