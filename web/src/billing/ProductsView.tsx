// The price list: the items a document line can be raised from. Same shape as
// the customer list on purpose — one module, one way to read a table.
//
// Prices are shown without a currency symbol: a price list is quoted in the
// tenant's own currency, and it is the document that carries the currency it
// was raised in (`docs/design/billing.md`).
import { useCallback, useEffect, useMemo, useState } from "react";
import { Package, Upload } from "lucide-react";

import { strings, useLocale } from "../i18n";
import { Badge, Button, Table, Td, Th, useDialogs } from "../ds";
import { billingMessage, useBillingApi } from "./api";
import { BillingPagination } from "./BillingPagination";
import { formatAmount, formatRate } from "./money";
import { ProductDialog } from "./ProductDialog";
import { PriceImportDialog } from "./PriceImportDialog";
import { BillingLoading, EmptyState, ErrorBanner, ListToolbar } from "./parts";
import type { BillingProduct } from "./types";
import styles from "./billingStyles";
import { useBillingPagination } from "./useBillingPagination";

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
  const [editing, setEditing] = useState<BillingProduct | null | undefined>(
    undefined,
  );
  const [importing, setImporting] = useState(false);

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
      (p) =>
        needle === "" || `${p.name} ${p.unit}`.toLowerCase().includes(needle),
    );
  }, [products, search]);
  const paged = useBillingPagination(shown, `${search}\u0000${includeArchived}`);

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
      <ListToolbar
        label={strings.billingProducts}
        search={search}
        onSearch={setSearch}
        searchLabel={strings.billingSearchProducts}
        includeArchived={includeArchived}
        onIncludeArchived={setIncludeArchived}
        createLabel={strings.billingNewProduct}
        onCreate={() => setEditing(null)}
        busy={loading}
        showCreate={products.length > 0}
        beforeCreate={<Button variant="ghost" icon={<Upload aria-hidden="true" />} onClick={() => setImporting(true)}>{strings.billingImportPrices}</Button>}
      />

      {error !== null && (
        <ErrorBanner
          message={error}
          presentation="popup"
          onDismiss={() => setError(null)}
        />
      )}

      {loading ? (
        <BillingLoading />
      ) : products.length === 0 ? (
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
        <><Table
          label={strings.billingProducts}
          className={styles.listTable}
          stickyHeader
          interactiveRows
        >
          <thead>
            <tr>
              <Th>{strings.billingColName}</Th>
              <Th>{strings.billingColUnit}</Th>
              <Th numeric>{strings.billingColUnitPrice}</Th>
              <Th numeric>{strings.billingColVatRate}</Th>
              <Th hideLabel>{strings.billingColActions}</Th>
            </tr>
          </thead>
          <tbody>
            {paged.records.map((p) => (
              <tr
                key={p.id}
                className={p.archived ? styles.archivedRow : undefined}
              >
                <td>
                  <button
                    type="button"
                    className={styles.rowName}
                    onClick={() => setEditing(p)}
                  >
                    {p.name}
                  </button>
                  {p.archived && (
                    <Badge className="ml-2 align-middle">
                      {strings.billingArchived}
                    </Badge>
                  )}
                </td>
                <td>{p.unit}</td>
                <Td numeric>{formatAmount(p.unitPriceCents, locale)}</Td>
                <Td numeric>{formatRate(p.vatRateBp, locale)}</Td>
                <td className={styles.rowActions}>
                  <button
                    type="button"
                    className={styles.linkAction}
                    onClick={() => void toggleArchived(p)}
                  >
                    {p.archived
                      ? strings.billingRestore
                      : strings.billingArchive}
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </Table><BillingPagination {...paged} onPage={paged.setPage} /></>
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
      {importing && <PriceImportDialog existing={products} onClose={() => setImporting(false)} onImported={() => { setImporting(false); void load(); }} />}
    </div>
  );
}
