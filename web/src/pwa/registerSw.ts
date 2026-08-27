// Registers the offline-shell service worker (public/sw.js) once the page has
// finished loading, so registration never competes with the app's own
// bootstrap for bandwidth. Production builds only: in dev the worker would
// sit between Vite and the browser — a moving part with nothing to protect.
//
// Calling register() on every page load doubles as the deploy-update check:
// the browser re-fetches sw.js, and on any byte change (a deploy bumped its
// VERSION) installs the new worker, which takes over immediately
// (skipWaiting + clients.claim in sw.js itself).

export function registerOfflineShell(win: Window, enabled: boolean): void {
  if (!enabled || !("serviceWorker" in win.navigator)) return;
  win.addEventListener("load", () => {
    win.navigator.serviceWorker.register("/sw.js").catch(() => {
      // Private windows and flaky first loads can refuse registration; the
      // cost is only the offline screen, never the app.
    });
  });
}
