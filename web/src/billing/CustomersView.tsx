// The customer list: who the tenant bills, and the only place a customer is
// created or edited. Archiving is the only removal — an issued invoice must
// always be able to name the party it was raised for — so the list has an
// archive action and an "include archived" view, never a delete.
import { useCallback, useEffect, useMemo, useState } from "react";
import { UserRound } from "lucide-react";

import { strings } from "../i18n";
import { Badge, Table, Th, useDialogs } from "../ds";
import { billingMessage, useBillingApi } from "./api";
import { BillingPagination } from "./BillingPagination";
import { CustomerDialog } from "./CustomerDialog";
import { BillingLoading, EmptyState, ErrorBanner, ListToolbar } from "./parts";
import type { BillingCustomer } from "./types";
import styles from "./billingStyles";
import { useBillingPagination } from "./useBillingPagination";

/** Whether a customer answers the search box (name, city, country, VAT id). */
function matches(c: BillingCustomer, needle: string): boolean {
  if (needle === "") return true;
  const hay = [c.name, c.city, c.country, c.vatId ?? "", c.email ?? ""]
    .join(" ")
    .toLowerCase();
  return hay.includes(needle);
}

export function CustomersView() {
  const api = useBillingApi();
  const { confirm } = useDialogs();
  const [customers, setCustomers] = useState<BillingCustomer[]>([]);
  const [includeArchived, setIncludeArchived] = useState(false);
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  /** `undefined` = closed, `null` = creating, a record = editing it. */
  const [editing, setEditing] = useState<BillingCustomer | null | undefined>(
    undefined,
  );

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setCustomers(await api.customers(includeArchived));
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
    return customers.filter((c) => matches(c, needle));
  }, [customers, search]);
  const paged = useBillingPagination(shown, `${search}\u0000${includeArchived}`);

  async function toggleArchived(customer: BillingCustomer) {
    if (
      !customer.archived &&
      !(await confirm({
        title: strings.billingArchive,
        message: strings.billingArchiveCustomerConfirm(customer.name),
        confirmLabel: strings.billingArchive,
      }))
    ) {
      return;
    }
    try {
      await api.setCustomerArchived(customer.id, !customer.archived);
      await load();
    } catch (err) {
      setError(billingMessage(err, strings.billingSaveFailed));
    }
  }

  return (
    <div className={styles.page}>
      <ListToolbar
        label={strings.billingCustomers}
        search={search}
        onSearch={setSearch}
        searchLabel={strings.billingSearchCustomers}
        includeArchived={includeArchived}
        onIncludeArchived={setIncludeArchived}
        createLabel={strings.billingNewCustomer}
        onCreate={() => setEditing(null)}
        busy={loading}
        showCreate={customers.length > 0}
      />

      {error !== null && <ErrorBanner message={error} />}

      {loading ? (
        <BillingLoading />
      ) : customers.length === 0 ? (
        <div className={styles.customerEmptyLayout}>
          <div className={styles.customerEmptyCard}>
            <EmptyState
              Icon={UserRound}
              title={strings.billingNoCustomersTitle}
              body={strings.billingNoCustomersBody}
              cta={strings.billingNewCustomer}
              onCta={() => setEditing(null)}
            />
          </div>
        </div>
      ) : shown.length === 0 ? (
        <p className={styles.noMatches}>{strings.billingNoMatches}</p>
      ) : (
        <><Table
          label={strings.billingCustomers}
          className={styles.listTable}
          stickyHeader
          interactiveRows
        >
          <thead>
            <tr>
              <Th>{strings.billingColName}</Th>
              <Th>{strings.billingColLocation}</Th>
              <Th>{strings.billingColVatId}</Th>
              <Th>{strings.billingColEmail}</Th>
              <Th>{strings.billingColTerms}</Th>
              <Th>{strings.billingColCurrency}</Th>
              <Th hideLabel>{strings.billingColActions}</Th>
            </tr>
          </thead>
          <tbody>
            {paged.records.map((c) => (
              <tr
                key={c.id}
                className={c.archived ? styles.archivedRow : undefined}
              >
                <td>
                  <button
                    type="button"
                    className={styles.rowName}
                    onClick={() => setEditing(c)}
                  >
                    {c.name}
                  </button>
                  {c.archived && (
                    <Badge className="ml-2 align-middle">
                      {strings.billingArchived}
                    </Badge>
                  )}
                </td>
                <td>
                  {[c.city, c.country].filter((v) => v !== "").join(", ")}
                </td>
                <td className={styles.mono}>{c.vatId ?? ""}</td>
                <td>{c.email ?? ""}</td>
                <td>{strings.billingTermsDays(c.paymentTermsDays)}</td>
                <td>{c.currency}</td>
                <td className={styles.rowActions}>
                  <button
                    type="button"
                    className={styles.linkAction}
                    onClick={() => void toggleArchived(c)}
                  >
                    {c.archived
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
        <CustomerDialog
          customer={editing}
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
