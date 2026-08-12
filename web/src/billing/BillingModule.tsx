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
import { Building2, FileText, IdCard, Package, ReceiptText, RefreshCw, Tags } from "lucide-react";

import { strings } from "../i18n";
import { CustomersView } from "./CustomersView";
import { InvoiceEditor } from "./InvoiceEditor";
import { InvoicesView } from "./InvoicesView";
import { ProductsView } from "./ProductsView";
import { QuoteEditor } from "./QuoteEditor";
import { QuotesView } from "./QuotesView";
import { SchedulesView } from "./SchedulesView";
import { VatReportView } from "./VatReportView";
import { SettingsView } from "./SettingsView";
import styles from "./BillingModule.module.css";

/** The tabs: the documents that are the point of the module, then the offers
 *  that become them, then who they are made out to, then what they are made
 *  of. Invoices stay first — and stay what `/billing` lands on — because they
 *  are what a tenant opens billing to look at. */
const TABS = [
  { path: "customers", label: () => strings.billingCustomers, Icon: Building2 },
  { path: "products", label: () => strings.billingProducts, Icon: Tags },
  { path: "quotes", label: () => strings.billingQuotes, Icon: FileText },
  { path: "invoices", label: () => strings.billingInvoices, Icon: ReceiptText },
  // What bills itself again on a rhythm (B2.11). After the documents it
  // raises, because that is what it produces — a reader looks for the invoice
  // first and the arrangement behind it second.
  { path: "recurring", label: () => strings.billingRecurring, Icon: RefreshCw },
  // The figures a VAT return is copied from (B1.20). After the records it is
  // computed from, because it is a read over them rather than a thing a tenant
  // keeps.
  { path: "reports", label: () => strings.billingReports, Icon: Package },
  // Last, and deliberately not first: it is filled in once and then printed on
  // every document, so it belongs beside the records rather than in front of
  // them. What it holds is who the tenant invoices AS (B1.16).
  { path: "settings", label: () => strings.billingSettings, Icon: IdCard },
] as const;

const billingPath = (path: (typeof TABS)[number]["path"]) => `/billing/${path}`;

export function BillingModule() {
  return (
    <div className={styles.billing}>
      <header className={styles.header}>
        <h1 className={styles.title}>{strings.moduleBilling}</h1>
        <nav className={styles.tabs}>
          {TABS.map((t) => (
            <NavLink
              key={t.path}
              to={billingPath(t.path)}
              className={({ isActive }) => (isActive ? `${styles.tab} ${styles.tabActive}` : styles.tab)}
            >
              <t.Icon aria-hidden="true" />
              {t.label()}
            </NavLink>
          ))}
        </nav>
      </header>

      <Routes>
        <Route index element={<Navigate to="/billing/customers" replace />} />
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
        <Route path="recurring" element={<SchedulesView />} />
        <Route path="customers" element={<CustomersView />} />
        <Route path="products" element={<ProductsView />} />
        <Route path="reports" element={<VatReportView />} />
        <Route path="settings" element={<SettingsView />} />
        {/* An unknown billing path is a stale link, not an error page. */}
        <Route path="*" element={<Navigate to="/billing/customers" replace />} />
      </Routes>
    </div>
  );
}
