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
import {
  Building2,
  FileText,
  IdCard,
  Package,
  PlugZap,
  ReceiptText,
  RefreshCw,
  Tags,
  WalletCards,
} from "lucide-react";

import { strings } from "../i18n";
import { CustomersView } from "./CustomersView";
import { InvoiceEditor } from "./InvoiceEditor";
import { InvoicesView } from "./InvoicesView";
import { ProductsView } from "./ProductsView";
import { PriceConnectionsView } from "./PriceConnectionsView";
import { QuoteEditor } from "./QuoteEditor";
import { QuotesView } from "./QuotesView";
import { SchedulesView } from "./SchedulesView";
import { VatReportView } from "./VatReportView";
import { SettingsView } from "./SettingsView";

/** The tabs: the documents that are the point of the module, then the offers
 *  that become them, then who they are made out to, then what they are made
 *  of. Invoices stay first — and stay what `/billing` lands on — because they
 *  are what a tenant opens billing to look at. */
const TABS = [
  { path: "customers", label: () => strings.billingCustomers, Icon: Building2 },
  { path: "products", label: () => strings.billingProducts, Icon: Tags },
  {
    path: "connections",
    label: () => strings.billingPriceConnections,
    Icon: PlugZap,
  },
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
  { path: "details", label: () => strings.billingSettings, Icon: IdCard },
] as const;

const billingPath = (path: (typeof TABS)[number]["path"]) => `/billing/${path}`;

/** Billing opens on the document people return to most often. Kept as a
 * named contract so a shell refactor cannot quietly send the rail entry back
 * to setup data. */
export const BILLING_DEFAULT_PATH = "/billing/invoices";

export function BillingModule() {
  return (
    <div className="flex h-full min-h-0 w-full flex-col bg-app">
      <header className="shrink-0 border-b border-subtle bg-surface px-8 pb-4 pt-6 max-sm:px-4 max-sm:pt-4">
        <div className="flex items-center gap-3.5">
          <span
            className="flex size-12 shrink-0 items-center justify-center rounded-2xl bg-[var(--accent-soft)] text-accent shadow-sm ring-1 ring-inset ring-accent/10"
            aria-hidden="true"
          >
            <WalletCards className="size-5" />
          </span>
          <div className="min-w-0">
            <h1 className="m-0 text-2xl font-bold tracking-tight text-primary">
              {strings.moduleBilling}
            </h1>
            <p className="m-0 mt-1 text-sm text-secondary">
              {strings.billingWorkspacePurpose}
            </p>
          </div>
        </div>
        <nav
          className="mt-5 flex min-w-0 gap-2 overflow-x-auto"
          aria-label={strings.moduleBilling}
        >
          {TABS.map((t) => (
            <NavLink
              key={t.path}
              to={billingPath(t.path)}
              className={({ isActive }) =>
                `inline-flex min-h-11 shrink-0 items-center gap-2.5 rounded-xl px-4 py-2.5 text-sm !no-underline transition-colors hover:!no-underline focus-visible:!no-underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent ${
                  isActive
                    ? "bg-[var(--accent-soft)] font-semibold !text-accent shadow-sm ring-1 ring-inset ring-accent/10"
                    : "bg-transparent font-medium !text-secondary hover:bg-raised hover:!text-primary"
                }`
              }
            >
              <t.Icon className="size-4" aria-hidden="true" />
              {t.label()}
            </NavLink>
          ))}
        </nav>
      </header>

      <Routes>
        <Route index element={<Navigate to={BILLING_DEFAULT_PATH} replace />} />
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
        <Route path="connections" element={<PriceConnectionsView />} />
        <Route path="reports" element={<VatReportView />} />
        <Route path="details" element={<SettingsView />} />
        {/* Keep old bookmarks working while exposing a clear, user-facing URL. */}
        <Route
          path="settings"
          element={<Navigate to="/billing/details" replace />}
        />
        {/* An unknown billing path is a stale link, not an error page. */}
        <Route
          path="*"
          element={<Navigate to="/billing/customers" replace />}
        />
      </Routes>
    </div>
  );
}
