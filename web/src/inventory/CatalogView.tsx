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
import { Archive, ArchiveRestore, Package, Pencil, Plus, ScanLine } from "lucide-react";

import {
  ProductDialog,
  billingMessage,
  formatAmount,
  formatRate,
  useBillingApi,
  type BillingProduct,
} from "../billing";
import {
  Button,
  Checkbox,
  IconButton,
  Input,
  Spinner,
  Table,
  Td,
  Th,
  Toolbar,
  ToolbarSpacer,
  useDialogs,
} from "../ds";
import { getLocale, strings } from "../i18n";
import { inventoryMessage, useInventoryApi } from "./api";
import { qtyLabel } from "./format";
import { EmptyState, ErrorBanner } from "./parts";
import { ScanInput } from "./ScanInput";
import type { InvSupplier } from "./types";
import styles from "./InventoryModule.module.css";

/** What is being edited: an existing product, or the new one being added —
 *  which the scanner opens with the code it just read (B5.09c), so a thing
 *  nobody has catalogued yet is added by scanning it once. */
type Editing = { product: BillingProduct | null; barcode?: string };

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
  const [scanning, setScanning] = useState(false);
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
    <div className={`${styles.page} ${styles.catalogPage}`}>
      <section className={styles.catalogWorkspace}>
        <div className={styles.pageHeading}>
          <div className={styles.pageHeadingCopy}>
            <h2 className={styles.pageTitle}>{strings.inventoryTabCatalog}</h2>
            <p className={styles.pageSubtitle}>{strings.inventoryCatalogPurpose}</p>
          </div>
          <div className={styles.headerActions}>
            <Button variant="ghost" icon={<ScanLine size={16} />} onClick={() => setScanning(true)}>
              {strings.inventoryScan}
            </Button>
            {products.length > 0 && (
              <Button icon={<Plus size={16} />} onClick={() => setEditing({ product: null })}>
                {strings.inventoryNewProduct}
              </Button>
            )}
          </div>
        </div>
      <Toolbar label={strings.inventoryTabCatalog} surface="plain" className={styles.catalogFilters}>
        {/* The stylesheet's `flex: 0 1 260px`, taking the whole row on a
            phone so the action buttons can share the next one. */}
        <Input
          className="basis-[260px] max-[48rem]:basis-full"
          type="search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={strings.inventorySearchCatalog}
          aria-label={strings.inventorySearchCatalog}
        />
        <Checkbox
          checked={stockedOnly}
          onChange={setStockedOnly}
          label={strings.inventoryStockedOnly}
        />
        <Checkbox
          checked={includeArchived}
          onChange={setIncludeArchived}
          label={strings.inventoryShowArchived}
        />
        <ToolbarSpacer />
        {loading && <Spinner size={16} />}
      </Toolbar>
      </section>

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
        <Table label={strings.inventoryTabCatalog} interactiveRows>
          <thead>
            <tr>
              <Th>{strings.inventoryColProduct}</Th>
              <Th>{strings.inventoryColSku}</Th>
              <Th>{strings.inventoryColBarcode}</Th>
              <Th numeric>{strings.inventoryColOnHand}</Th>
              <Th numeric>{strings.inventoryColPurchasePrice}</Th>
              <Th numeric>{strings.inventoryColSalePrice}</Th>
              <Th numeric>{strings.inventoryColVatRate}</Th>
              <Th hideLabel>{strings.inventoryColActions}</Th>
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
                <Td numeric>
                  {product.stocked ? (
                    qtyLabel(onHand.get(product.id) ?? 0)
                  ) : (
                    <span className={styles.muted}>{strings.inventoryNotStocked}</span>
                  )}
                </Td>
                <Td numeric>{formatAmount(product.purchasePriceCents, locale)}</Td>
                <Td numeric>{formatAmount(product.unitPriceCents, locale)}</Td>
                <Td numeric>{formatRate(product.vatRateBp, locale)}</Td>
                <td className={styles.rowActions}>
                  <IconButton
                    label={strings.inventoryEdit}
                    icon={<Pencil />}
                    onClick={() => setEditing({ product })}
                  />
                  <IconButton
                    label={product.archived ? strings.inventoryRestore : strings.inventoryArchive}
                    icon={product.archived ? <ArchiveRestore /> : <Archive />}
                    onClick={() => void toggleArchived(product)}
                  />
                </td>
              </tr>
            ))}
          </tbody>
        </Table>
      )}

      {scanning && (
        <ScanInput
          onClose={() => setScanning(false)}
          // The scanned product is opened in the same editor as a clicked one:
          // the answer to "what is this box" is the record, and a second view
          // of it would drift from this one.
          action={{
            label: strings.inventoryScanOpenProduct,
            run: (found) => {
              setScanning(false);
              setEditing({ product: found.product });
            },
          }}
          // A real code nobody carries is the catalog's own empty state: the
          // thing in somebody's hand is added by scanning it, with the digits
          // already filled in.
          onUnknown={{
            label: strings.inventoryScanAddProduct,
            run: (code) => {
              setScanning(false);
              setEditing({ product: null, barcode: code });
            },
          }}
        />
      )}

      {editing !== null && (
        <ProductDialog
          product={editing.product}
          initialBarcode={editing.barcode}
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
