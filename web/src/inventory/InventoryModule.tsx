// The Inventory module (alo Inventory, ADR 0035, wave B5) — the workspace
// surface over the `/inventory` API and the catalog half of `/billing/products`.
//
// It is mounted at `/inventory/*` by the product surface, so every path below is
// relative and a deep link survives a page reload.
//
// **Two tabs today, and the rest are not hidden — they are not built yet.** This
// item (B5.09a) ships the catalog and the stock list with a product's movement
// history behind a click. Purchasing, sales orders, shortages, the stocktake and
// the scanner are B5.09b/c and B5.10 in `docs/autonomy/QUEUE.md`; a tab that
// only apologised for itself would be worse than a module that grows one tab per
// item. The routes they will need are already served (`server.rs`) — what is
// missing here is the screen, and nothing else.
import { NavLink, Navigate, Route, Routes } from "react-router-dom";

import { strings } from "../i18n";
import { CatalogView } from "./CatalogView";
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
 *  then how much of it there is. */
const TABS: { path: string; label: () => string }[] = [
  { path: "catalog", label: () => strings.inventoryTabCatalog },
  { path: "stock", label: () => strings.inventoryTabStock },
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
        {/* An unknown Inventory path is a stale link, not an error page. */}
        <Route path="*" element={<Navigate to={`${INVENTORY_ROOT}/catalog`} replace />} />
      </Routes>
    </div>
  );
}
