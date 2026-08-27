// The alo offline-shell service worker (served verbatim from public/).
//
// Scope, on purpose: it precaches ONE thing — the self-contained offline
// screen — and intercepts ONLY top-level navigations, network-first. While
// the network answers, every response (the app shell, OAuth redirects, API
// calls) passes through untouched and uncached; only when the network is
// unreachable does a navigation get the honest offline screen instead of the
// browser's error page. Nothing else is ever cached: a stale app shell served
// from cache is worse than no PWA, and mail data offline is a sync engine,
// not a cache (both cuts recorded in docs/autonomy/mail/STATE.md).
//
// Updates: any byte change here makes the browser install the new worker on
// the next visit; skipWaiting + clients.claim make it take over immediately,
// and activate drops every previous version's cache. Bump VERSION whenever
// offline.html changes — that page is precached under this version, so an
// unbumped edit would keep serving the old copy to existing installs.

const VERSION = "v1";
const CACHE_NAME = "alo-offline-" + VERSION;
const OFFLINE_URL = "/offline.html";

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(CACHE_NAME)
      // "reload" bypasses the HTTP cache so the precached copy is the
      // deployed one, not whatever the browser had lying around.
      .then((cache) => cache.add(new Request(OFFLINE_URL, { cache: "reload" })))
      .then(() => self.skipWaiting()),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) =>
        Promise.all(
          keys
            .filter((key) => key.startsWith("alo-offline-") && key !== CACHE_NAME)
            .map((key) => caches.delete(key)),
        ),
      )
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  // Navigations only. API calls, asset loads, auth redirects and everything
  // else never enter this worker — they go straight to the network.
  if (request.method !== "GET" || request.mode !== "navigate") return;
  event.respondWith(
    fetch(request).catch(() =>
      caches.match(OFFLINE_URL).then(
        (cached) =>
          cached ??
          // The precache failed or was evicted: a plain answer beats a hang.
          new Response("offline", {
            status: 503,
            headers: { "Content-Type": "text/plain" },
          }),
      ),
    ),
  );
});
