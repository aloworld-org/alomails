// The Billing module (alo Billing, ADR 0035, wave B1) — the workspace surface
// over the `/billing` API: a header, a tab per record type, and a nested route
// each. Documents come first, because they are what the module is for; the
// customers and the price list are what a document is assembled from.
//
// It is mounted at `/billing/*` by the product surface, so every path below is
// relative and a deep link (`/billing/invoices/{id}`) survives a page reload.
// The document routes are nested under one `invoices` parent rather than
// spelled out as two flat paths, so that a relative `..` from the editor lands
// on the list instead of on the module root.
import { NavLink, Navigate, Route, Routes } from "react-router-dom";

import { strings } from "../i18n";
import { CustomersView } from "./CustomersView";
import { InvoiceEditor } from "./InvoiceEditor";
import { InvoicesView } from "./InvoicesView";
import { ProductsView } from "./ProductsView";
import { QuoteEditor } from "./QuoteEditor";
import { QuotesView } from "./QuotesView";
import { SettingsView } from "./SettingsView";
import styles from "./BillingModule.module.css";

/** The tabs: the documents that are the point of the module, then the offers
 *  that become them, then who they are made out to, then what they are made
 *  of. Invoices stay first — and stay what `/billing` lands on — because they
 *  are what a tenant opens billing to look at. */
const TABS = [
  { path: "invoices", label: () => strings.billingInvoices },
  { path: "quotes", label: () => strings.billingQuotes },
  { path: "customers", label: () => strings.billingCustomers },
  { path: "products", label: () => strings.billingProducts },
  // Last, and deliberately not first: it is filled in once and then printed on
  // every document, so it belongs beside the records rather than in front of
  // them. What it holds is who the tenant invoices AS (B1.16).
  { path: "settings", label: () => strings.billingSettings },
] as const;

export function BillingModule() {
  return (
    <div className={styles.billing}>
      <header className={styles.header}>
        <h1 className={styles.title}>{strings.moduleBilling}</h1>
        <nav className={styles.tabs}>
          {TABS.map((t) => (
            <NavLink
              key={t.path}
              to={t.path}
              className={({ isActive }) => (isActive ? `${styles.tab} ${styles.tabActive}` : styles.tab)}
            >
              {t.label()}
            </NavLink>
          ))}
        </nav>
      </header>

      <Routes>
        <Route index element={<Navigate to="invoices" replace />} />
        <Route path="invoices">
          <Route index element={<InvoicesView />} />
          <Route path="new" element={<InvoiceEditor />} />
          <Route path=":id" element={<InvoiceEditor />} />
        </Route>
        <Route path="quotes">
          <Route index element={<QuotesView />} />
          <Route path="new" element={<QuoteEditor />} />
          <Route path=":id" element={<QuoteEditor />} />
        </Route>
        <Route path="customers" element={<CustomersView />} />
        <Route path="products" element={<ProductsView />} />
        <Route path="settings" element={<SettingsView />} />
        {/* An unknown billing path is a stale link, not an error page. */}
        <Route path="*" element={<Navigate to="invoices" replace />} />
      </Routes>
    </div>
  );
}
