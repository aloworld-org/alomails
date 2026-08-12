// The customer list: who the tenant bills, and the only place a customer is
// created or edited. Archiving is the only removal — an issued invoice must
// always be able to name the party it was raised for — so the list has an
// archive action and an "include archived" view, never a delete.
import { useCallback, useEffect, useMemo, useState } from "react";
import { Building2, CircleDollarSign, FileText, UserRound } from "lucide-react";

import { strings } from "../i18n";
import { useDialogs } from "../ds";
import { billingMessage, useBillingApi } from "./api";
import { CustomerDialog } from "./CustomerDialog";
import { BillingLoading, EmptyState, ErrorBanner, Toolbar } from "./parts";
import type { BillingCustomer } from "./types";
import styles from "./BillingModule.module.css";

/** Whether a customer answers the search box (name, city, country, VAT id). */
function matches(c: BillingCustomer, needle: string): boolean {
  if (needle === "") return true;
  const hay = [c.name, c.city, c.country, c.vatId ?? "", c.email ?? ""].join(" ").toLowerCase();
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
  const [editing, setEditing] = useState<BillingCustomer | null | undefined>(undefined);

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
      <Toolbar
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

      {loading ? <BillingLoading /> : customers.length === 0 ? (
        <div className={styles.customerEmptyLayout}>
          <div className={styles.customerEmptyCard}>
            <EmptyState Icon={Building2} title={strings.billingNoCustomersTitle} body={strings.billingNoCustomersBody} cta={strings.billingNewCustomer} onCta={() => setEditing(null)} />
          </div>
          <section className={styles.getStarted} aria-labelledby="billing-get-started">
            <h2 id="billing-get-started">{strings.billingGetStarted}</h2>
            <ol>
              <li><span>1</span><UserRound aria-hidden="true" /><div><strong>{strings.billingStepCustomerTitle}</strong><p>{strings.billingStepCustomerBody}</p></div></li>
              <li><span>2</span><FileText aria-hidden="true" /><div><strong>{strings.billingStepInvoiceTitle}</strong><p>{strings.billingStepInvoiceBody}</p></div></li>
              <li><span>3</span><CircleDollarSign aria-hidden="true" /><div><strong>{strings.billingStepPaidTitle}</strong><p>{strings.billingStepPaidBody}</p></div></li>
            </ol>
          </section>
        </div>
      ) : shown.length === 0 ? (
        <p className={styles.noMatches}>{strings.billingNoMatches}</p>
      ) : (
        <div className={styles.tableWrap}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th scope="col">{strings.billingColName}</th>
                <th scope="col">{strings.billingColLocation}</th>
                <th scope="col">{strings.billingColVatId}</th>
                <th scope="col">{strings.billingColEmail}</th>
                <th scope="col">{strings.billingColTerms}</th>
                <th scope="col">{strings.billingColCurrency}</th>
                <th scope="col">
                  <span className={styles.srOnly}>{strings.billingColActions}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {shown.map((c) => (
                <tr key={c.id} className={c.archived ? styles.archivedRow : undefined}>
                  <td>
                    <button
                      type="button"
                      className={styles.rowName}
                      onClick={() => setEditing(c)}
                    >
                      {c.name}
                    </button>
                    {c.archived && <span className={styles.badge}>{strings.billingArchived}</span>}
                  </td>
                  <td>{[c.city, c.country].filter((v) => v !== "").join(", ")}</td>
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
                      {c.archived ? strings.billingRestore : strings.billingArchive}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
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
