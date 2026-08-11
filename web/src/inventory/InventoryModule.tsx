// The Inventory module (alo Inventory, ADR 0035, wave B5) — the workspace
// surface over the `/inventory` API and the catalog half of `/billing/products`.
//
// It is mounted at `/inventory/*` by the product surface, so every path below is
// relative and a deep link survives a page reload.
//
// **Four tabs today, and the rest are not hidden — they are not built yet.**
// B5.09a shipped the catalog and the stock list with a product's movement
// history behind a click; B5.09b adds the two order documents and the acts that
// move goods through them. The scanner, the shortage list and the stocktake are
// B5.09c and later in `docs/autonomy/QUEUE.md`; a tab that only apologised for
// itself would be worse than a module that grows one tab per item. The routes
// they will need are already served (`server.rs`) — what is missing is the
// screen, and nothing else.
import { NavLink, Navigate, Route, Routes } from "react-router-dom";

import { strings } from "../i18n";
import { CatalogView } from "./CatalogView";
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
 *  how much of it there is, and then the two documents that change that number
 *  — what we have asked for, and what we have promised. */
const TABS: { path: string; label: () => string }[] = [
  { path: "catalog", label: () => strings.inventoryTabCatalog },
  { path: "stock", label: () => strings.inventoryTabStock },
  { path: "purchase-orders", label: () => strings.inventoryTabPurchasing },
  { path: "sales-orders", label: () => strings.inventoryTabSales },
];

export function InventoryModule() {
  return (
    <div className={styles.inventory}>
      <header className={styles.header}>
        <h1 className={styles.title}>{strings.moduleInventory}</h1>
        <nav className={styles.tabs}>
          {TABS.map((tab) => (
            <NavLink
              key={tab.path}
              to={`${INVENTORY_ROOT}/${tab.path}`}
              className={({ isActive }) =>
                isActive ? `${styles.tab} ${styles.tabActive}` : styles.tab
              }
            >
              {tab.label()}
            </NavLink>
          ))}
        </nav>
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
        {/* An unknown Inventory path is a stale link, not an error page. */}
        <Route path="*" element={<Navigate to={`${INVENTORY_ROOT}/catalog`} replace />} />
      </Routes>
    </div>
  );
}
