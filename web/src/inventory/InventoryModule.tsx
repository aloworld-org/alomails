// The Inventory module (alo Inventory, ADR 0035, wave B5) — the workspace
// surface over the `/inventory` API and the catalog half of `/billing/products`.
//
// It is mounted at `/inventory/*` by the product surface, so every path below is
// relative and a deep link survives a page reload.
//
// **Four tabs today, and the rest are not hidden — they are not built yet.**
// B5.09a shipped the catalog and the stock list with a product's movement
// history behind a click; B5.09b adds the two order documents and the acts that
// move goods through them. The shortage list and the stocktake are later items
// in `docs/autonomy/QUEUE.md`; a tab that only apologised for itself would be
// worse than a module that grows one tab per item. The routes they will need
// are already served (`server.rs`) — what is missing is the screen, and nothing
// else.
//
// **The scanner (B5.09c) is not a tab and should not be**: reading the code on
// a box is how a person addresses a product, not a place they go. It is a
// button on the two screens that are about products — the catalog and the stock
// list — and each says what to do with what was scanned.
import { NavLink, Navigate, Route, Routes } from "react-router-dom";
import { Boxes, ClipboardList, PackageSearch, ShoppingCart, Warehouse } from "lucide-react";

import { ModuleNavigation, moduleNavigationItemClassName } from "../ds";
import { strings } from "../i18n";
import { CatalogView } from "./CatalogView";
import { OrderBookView } from "./OrderBookView";
import { PurchaseOrderEditor } from "./PurchaseOrderEditor";
import { PurchaseOrdersView } from "./PurchaseOrdersView";
import { SalesOrderEditor } from "./SalesOrderEditor";
import { SalesOrdersView } from "./SalesOrdersView";
import { StockView } from "./StockView";
import styles from "./InventoryModule.module.css";

/**
 * Where the product surface mounts this module (`product/workplace.tsx`).
 *
 * **Every link and redirect below is absolute, and this is why.** The module is
 * mounted on a splat route (`/inventory/*`), and react-router resolves a
 * relative `to` inside one against the *current location* rather than against
 * the route — so `to="stock"` read from `/inventory/catalog` navigates to
 * `/inventory/catalog/stock`, which matches the catch-all, which redirects
 * relatively again, and the address grows a segment per render. Finance learned
 * this the same way (B4.13a).
 */
const INVENTORY_ROOT = "/inventory";

/** The tabs, in the order the module is worked through: what we sell and stock,
 *  how much of it there is, then the two documents that change that number —
 *  what we have asked for, and what we have promised — and last the book that
 *  reads across every one of the latter at once. */
const TABS = [
  { path: "catalog", label: () => strings.inventoryTabCatalog, Icon: PackageSearch },
  { path: "stock", label: () => strings.inventoryTabStock, Icon: Warehouse },
  { path: "purchase-orders", label: () => strings.inventoryTabPurchasing, Icon: ShoppingCart },
  { path: "sales-orders", label: () => strings.inventoryTabSales, Icon: ClipboardList },
  { path: "order-book", label: () => strings.inventoryTabOrderBook, Icon: Boxes },
];

export function InventoryModule() {
  return (
    <div className={styles.inventory}>
      <header className="shrink-0 border-b border-subtle bg-header px-8 pb-3 pt-5 max-sm:px-4 max-sm:pt-4">
        <div className="flex items-center gap-3">
          <span className="flex size-11 shrink-0 items-center justify-center rounded-xl bg-[var(--accent-soft)] text-accent ring-1 ring-inset ring-accent/10" aria-hidden="true">
            <Boxes className="size-5" />
          </span>
          <div className="min-w-0">
            <h1 className="m-0 text-2xl font-bold tracking-tight text-primary">{strings.moduleInventory}</h1>
            <p className="m-0 mt-1 text-sm text-secondary">{strings.inventoryWorkspacePurpose}</p>
          </div>
        </div>
        {/* Scrolls horizontally on a phone by design; the responsive e2e
            sweep exempts marked strips from its width invariant. */}
        <ModuleNavigation className="mt-4 gap-1" label={strings.moduleInventory} data-allow-overflow="">
          {TABS.map((tab) => (
            <NavLink
              key={tab.path}
              to={`${INVENTORY_ROOT}/${tab.path}`}
              className={({ isActive }) => moduleNavigationItemClassName(isActive)}
            >
              <tab.Icon className="size-4" aria-hidden="true" />
              {tab.label()}
            </NavLink>
          ))}
        </ModuleNavigation>
      </header>

      <Routes>
        <Route index element={<Navigate to={`${INVENTORY_ROOT}/catalog`} replace />} />
        <Route path="catalog" element={<CatalogView />} />
        <Route path="stock" element={<StockView />} />
        {/* A document is a page and not a drawer: it is long, it is printed,
            and a person sends its address to a colleague. */}
        <Route path="purchase-orders">
          <Route index element={<PurchaseOrdersView />} />
          <Route path="new" element={<PurchaseOrderEditor />} />
          <Route path=":id" element={<PurchaseOrderEditor />} />
        </Route>
        <Route path="sales-orders">
          <Route index element={<SalesOrdersView />} />
          <Route path="new" element={<SalesOrderEditor />} />
          <Route path=":id" element={<SalesOrderEditor />} />
        </Route>
        {/* A read across every sales order at once, and the only screen here
            with no acts of its own — each row is a way into the document. */}
        <Route path="order-book" element={<OrderBookView />} />
        {/* An unknown Inventory path is a stale link, not an error page. */}
        <Route path="*" element={<Navigate to={`${INVENTORY_ROOT}/catalog`} replace />} />
      </Routes>
    </div>
  );
}
