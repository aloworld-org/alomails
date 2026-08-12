// The price list: the items a document line can be raised from. Same shape as
// the customer list on purpose — one module, one way to read a table.
//
// Prices are shown without a currency symbol: a price list is quoted in the
// tenant's own currency, and it is the document that carries the currency it
// was raised in (`docs/design/billing.md`).
import { useCallback, useEffect, useMemo, useState } from "react";
import { Package } from "lucide-react";

import { strings, useLocale } from "../i18n";
import { useDialogs } from "../ds";
import { billingMessage, useBillingApi } from "./api";
import { formatAmount, formatRate } from "./money";
import { ProductDialog } from "./ProductDialog";
import { BillingLoading, EmptyState, ErrorBanner, Toolbar } from "./parts";
import type { BillingProduct } from "./types";
import styles from "./BillingModule.module.css";

export function ProductsView() {
  const api = useBillingApi();
  const locale = useLocale();
  const { confirm } = useDialogs();
  const [products, setProducts] = useState<BillingProduct[]>([]);
  const [includeArchived, setIncludeArchived] = useState(false);
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  /** `undefined` = closed, `null` = creating, a record = editing it. */
  const [editing, setEditing] = useState<BillingProduct | null | undefined>(undefined);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setProducts(await api.products(includeArchived));
      setError(null);
    } catch (err) {
      setError(billingMessage(err, strings.billingLoadFailed));
    } finally {
      setLoading(false);
    }
  }, [api, includeArchived]);

  useEffect(() => {
    void load();
  }, [load]);

  const shown = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return products.filter(
      (p) => needle === "" || `${p.name} ${p.unit}`.toLowerCase().includes(needle),
    );
  }, [products, search]);

  async function toggleArchived(product: BillingProduct) {
    if (
      !product.archived &&
      !(await confirm({
        title: strings.billingArchive,
        message: strings.billingArchiveProductConfirm(product.name),
        confirmLabel: strings.billingArchive,
      }))
    ) {
      return;
    }
    try {
      await api.setProductArchived(product.id, !product.archived);
      await load();
    } catch (err) {
      setError(billingMessage(err, strings.billingSaveFailed));
    }
  }

  return (
    <div className={styles.page}>
      <Toolbar
        search={search}
        onSearch={setSearch}
        searchLabel={strings.billingSearchProducts}
        includeArchived={includeArchived}
        onIncludeArchived={setIncludeArchived}
        createLabel={strings.billingNewProduct}
        onCreate={() => setEditing(null)}
        busy={loading}
      />

      {error !== null && <ErrorBanner message={error} />}

      {loading ? <BillingLoading /> : products.length === 0 ? (
        <EmptyState
          Icon={Package}
          title={strings.billingNoProductsTitle}
          body={strings.billingNoProductsBody}
          cta={strings.billingNewProduct}
          onCta={() => setEditing(null)}
        />
      ) : shown.length === 0 ? (
        <p className={styles.noMatches}>{strings.billingNoMatches}</p>
      ) : (
        <div className={styles.tableWrap}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th scope="col">{strings.billingColName}</th>
                <th scope="col">{strings.billingColUnit}</th>
                <th scope="col" className={styles.numeric}>
                  {strings.billingColUnitPrice}
                </th>
                <th scope="col" className={styles.numeric}>
                  {strings.billingColVatRate}
                </th>
                <th scope="col">
                  <span className={styles.srOnly}>{strings.billingColActions}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {shown.map((p) => (
                <tr key={p.id} className={p.archived ? styles.archivedRow : undefined}>
                  <td>
                    <button type="button" className={styles.rowName} onClick={() => setEditing(p)}>
                      {p.name}
                    </button>
                    {p.archived && <span className={styles.badge}>{strings.billingArchived}</span>}
                  </td>
                  <td>{p.unit}</td>
                  <td className={styles.numeric}>{formatAmount(p.unitPriceCents, locale)}</td>
                  <td className={styles.numeric}>{formatRate(p.vatRateBp, locale)}</td>
                  <td className={styles.rowActions}>
                    <button
                      type="button"
                      className={styles.linkAction}
                      onClick={() => void toggleArchived(p)}
                    >
                      {p.archived ? strings.billingRestore : strings.billingArchive}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {editing !== undefined && (
        <ProductDialog
          product={editing}
          onClose={() => setEditing(undefined)}
          onSaved={() => {
            setEditing(undefined);
            void load();
          }}
        />
      )}
    </div>
  );
}
