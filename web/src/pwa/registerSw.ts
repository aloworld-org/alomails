// Registers the offline-shell service worker (public/sw.js) once the page has
// finished loading, so registration never competes with the app's own
// bootstrap for bandwidth. Production builds only: in development, any stale
// Alo worker and offline cache are removed so they cannot mask Vite updates.
//
// Calling register() on every page load doubles as the deploy-update check:
// the browser re-fetches sw.js, and on any byte change (a deploy bumped its
// VERSION) installs the new worker, which takes over immediately
// (skipWaiting + clients.claim in sw.js itself).

const ALO_OFFLINE_CACHE_PREFIX = "alo-offline-";

function clearDevelopmentOfflineState(win: Window): void {
  const serviceWorker = win.navigator.serviceWorker;

  if (typeof serviceWorker.getRegistrations === "function") {
    void serviceWorker
      .getRegistrations()
      .then((registrations) =>
        Promise.all(registrations.map((registration) => registration.unregister())),
      )
      .catch(() => {
        // Cleanup is best-effort in private windows and restricted browsers.
      });
  }

  void win.caches
    ?.keys()
    .then((keys) =>
      Promise.all(
        keys
          .filter((key) => key.startsWith(ALO_OFFLINE_CACHE_PREFIX))
          .map((key) => win.caches.delete(key)),
      ),
    )
    .catch(() => {
      // A failed cleanup must never prevent the development app from loading.
    });
}

export function registerOfflineShell(win: Window, enabled: boolean): void {
  if (!("serviceWorker" in win.navigator)) return;

  if (!enabled) {
    clearDevelopmentOfflineState(win);
    return;
  }

  win.addEventListener("load", () => {
    win.navigator.serviceWorker.register("/sw.js").catch(() => {
      // Private windows and flaky first loads can refuse registration; the
      // cost is only the offline screen, never the app.
    });
  });
}
